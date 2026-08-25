#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-lib-adapter — library ecosystem adapter (MagiCore)
//! (ts → delegate WebAdapter npm-format; rust → orchestrate cargo Q10; python → pip passthrough)
//! (ponytail: rust/python version resolution = placeholder 0.1.0 khi dry; add khi save chạy tool native)

use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version,
    VersionRange,
};
use std::path::{Path, PathBuf};

// W6: SBOM support
use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomGenerator, SbomOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibLanguage {
    Ts,
    Rust,
    Python,
}

type ManifestProbe = fn(&Path) -> Option<String>;

pub struct LibAdapter {
    language: LibLanguage,
    web: Option<mgc_web_adapter::WebAdapter>,
}

fn detect_language(root: &Path) -> Option<LibLanguage> {
    let mgc_toml = root.join("mgc.toml");
    if let Ok(content) = std::fs::read_to_string(&mgc_toml) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco != "lib" && v.get("lib").is_none() {
                    return None;
                }
            }
            if let Some(lang) = v
                .get("lib")
                .and_then(|l| l.get("language"))
                .and_then(|l| l.as_str())
            {
                return match lang {
                    "ts" | "typescript" => Some(LibLanguage::Ts),
                    "rust" => Some(LibLanguage::Rust),
                    "python" => Some(LibLanguage::Python),
                    _ => None,
                };
            }
        }
    }
    if root.join("package.json").exists() {
        return Some(LibLanguage::Ts);
    }
    if root.join("Cargo.toml").exists() {
        return Some(LibLanguage::Rust);
    }
    if root.join("pyproject.toml").exists() {
        return Some(LibLanguage::Python);
    }
    None
}

fn manifest_is_lib(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "lib" {
                    return true;
                }
            }
            if v.get("lib").is_some() {
                return true;
            }
        }
    }
    let probes: [(&Path, ManifestProbe); 3] = [
        (&root.join("package.json"), probe_package_json),
        (&root.join("Cargo.toml"), probe_cargo_toml),
        (&root.join("pyproject.toml"), probe_pyproject),
    ];
    for (path, probe) in probes {
        if path.exists() {
            if let Some(eco) = probe(path) {
                if eco == "lib" {
                    return true;
                }
            }
        }
    }
    false
}

fn probe_package_json(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("magicore")
        .and_then(|m| m.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_cargo_toml(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("magicore"))
        .and_then(|mgc| mgc.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_pyproject(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("tool")
        .and_then(|t| t.get("magicore"))
        .and_then(|mgc| mgc.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run(cmd, args, &opts)
        .map_err(|e| mgc_types::MgError::Other(e.to_string()))?;
    Ok(())
}

/// pip package allowlist — đọc `[lib] pip_allowed_packages` từ mgc.toml (Q9).
/// Rỗng = fail-closed: mọi pip add/remove/update theo tên bị chặn, in cách khai báo.
fn read_pip_allowlist(root: &Path) -> Vec<String> {
    let mgc_toml = root.join("mgc.toml");
    let Ok(content) = std::fs::read_to_string(&mgc_toml) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&content) else {
        return Vec::new();
    };
    v.get("lib")
        .and_then(|l| l.get("pip_allowed_packages"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn check_pip_allowed(root: &Path, name: &str) -> MgResult<()> {
    let allowed = read_pip_allowlist(root);
    if allowed.iter().any(|a| a == name) {
        return Ok(());
    }
    Err(mgc_types::MgError::Other(format!(
        "pip '{}' is not in [lib].pip_allowed_packages (mgc.toml). Fail-closed — add the package there to allow pip install/uninstall.",
        name
    )))
}

impl LibAdapter {
    fn for_language(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
    ) -> Self {
        Self::for_language_with_chain(language, registry_url, token, &[])
    }

    fn for_language_with_chain(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
        fallbacks: &[(String, Option<String>)],
    ) -> Self {
        let web = if language == LibLanguage::Ts {
            Some(match (registry_url, token) {
                (Some(url), token) => {
                    mgc_web_adapter::WebAdapter::with_registry_chain(url, token, fallbacks.to_vec())
                }
                (None, _) => mgc_web_adapter::WebAdapter::new(),
            })
        } else {
            None
        };
        Self { language, web }
    }
}

fn parse_cargo_manifest(root: &Path) -> MgResult<Manifest> {
    mgc_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Lib)
}

fn write_cargo_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    mgc_adapter_base::cargo_manifest::write_manifest(root, manifest)
}

fn parse_pyproject_manifest(root: &Path) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(root.join("pyproject.toml"))
        .map_err(|e| mgc_types::MgError::Other(format!("read pyproject.toml: {e}")))?;
    let v: toml::Value = toml::from_str(&content)
        .map_err(|e| mgc_types::MgError::Other(format!("parse pyproject.toml: {e}")))?;
    let name = v
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();
    let mut manifest = Manifest::new(&name, Ecosystem::Lib);
    if let Some(deps) = v
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in deps {
            let spec = dep.as_str().unwrap_or_default();
            let dep = DependencySpec::parse(spec)?;
            manifest.add_dep(dep, false, false, false);
        }
    }
    Ok(manifest)
}

fn write_pyproject_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("pyproject.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| mgc_types::MgError::Other(format!("read pyproject.toml: {e}")))?;
    let mut v: toml::Value = toml::from_str(&content)
        .map_err(|e| mgc_types::MgError::Other(format!("parse pyproject.toml: {e}")))?;
    let project = v
        .as_table_mut()
        .and_then(|t| t.get_mut("project"))
        .and_then(|p| p.as_table_mut())
        .ok_or_else(|| mgc_types::MgError::Other("pyproject.toml missing [project]".to_string()))?;

    let mut deps: Vec<toml::Value> = Vec::new();
    for dep in manifest.dependencies.iter().filter(|d| !d.range.is_star()) {
        deps.push(toml::Value::String(format!(
            "{}>={}",
            dep.name.as_str(),
            dep.range
                .as_str()
                .trim_start_matches('^')
                .trim_start_matches('~')
                .trim_start_matches('=')
        )));
    }
    project.insert("dependencies".to_string(), toml::Value::Array(deps));

    std::fs::write(
        &path,
        toml::to_string_pretty(&v).map_err(|e| mgc_types::MgError::Other(e.to_string()))?,
    )
    .map_err(|e| mgc_types::MgError::Other(format!("write pyproject.toml: {e}")))?;
    Ok(())
}

/// rust: name → version từ Cargo.lock (không exec, đọc file chuẩn thôi).
fn cargo_lock_versions(root: &Path) -> Vec<(String, String)> {
    let Ok(content) = std::fs::read_to_string(root.join("Cargo.lock")) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&content) else {
        return Vec::new();
    };
    v.get("package")
        .and_then(|p| p.as_array())
        .map(|pkgs| {
            pkgs.iter()
                .filter_map(|p| {
                    let name = p.get("name")?.as_str()?.to_string();
                    let version = p.get("version")?.as_str()?.to_string();
                    Some((name, version))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// python: name → version từ venv site-packages *.dist-info/METADATA (không exec).
/// fallback: rỗng → list fall về manifest range.
fn dist_info_versions(root: &Path) -> Vec<(String, String)> {
    let candidates: [PathBuf; 4] = [
        root.join("venv").join("lib"),
        root.join(".venv").join("lib"),
        root.join("lib"),
        root.join("site-packages"),
    ];
    let mut out = Vec::new();
    for base in candidates {
        if !base.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_dist_infos(&path, &mut out);
                }
            }
        }
    }
    out
}

fn collect_dist_infos(dir: &Path, out: &mut Vec<(String, String)>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".dist-info") {
                if let Some((pkg, version)) = parse_dist_metadata(&path.join("METADATA")) {
                    out.push((pkg, version));
                }
            } else if path.is_dir() {
                collect_dist_infos(&path, out);
            }
        }
    }
}

/// Đọc Name + Version 2 dòng đầu METADATA.
fn parse_dist_metadata(path: &Path) -> Option<(String, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut version = None;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Version:") {
            version = Some(rest.trim().to_string());
        }
        if name.is_some() && version.is_some() {
            break;
        }
    }
    Some((name?, version?))
}

fn placeholder_id(name: &PackageName, range: Option<&VersionRange>) -> PackageId {
    let version = range
        .and_then(|r| r.satisfying_version())
        .unwrap_or_else(|| Version::new(0, 1, 0));
    PackageId::new(name.clone(), version)
}

fn version_from_manifest(
    root: &Path,
    name: &PackageName,
    language: LibLanguage,
) -> Option<Version> {
    let manifest = match language {
        LibLanguage::Rust => parse_cargo_manifest(root).ok()?,
        LibLanguage::Python => parse_pyproject_manifest(root).ok()?,
        LibLanguage::Ts => return None,
    };
    manifest
        .find_dep(name.as_str())
        .and_then(|d| d.range.satisfying_version())
}

#[async_trait]
impl PackageAdapter for LibAdapter {
    fn name(&self) -> &str {
        "lib"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Lib
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_lib(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        if let Some(web) = &self.web {
            return web.parse_manifest(project_root).await;
        }
        match self.language {
            LibLanguage::Rust => parse_cargo_manifest(project_root),
            LibLanguage::Python => parse_pyproject_manifest(project_root),
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.write_manifest(project_root, manifest).await;
        }
        match self.language {
            LibLanguage::Rust => write_cargo_manifest(project_root, manifest),
            LibLanguage::Python => write_pyproject_manifest(project_root, manifest),
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        if let Some(web) = &self.web {
            return web.resolve(manifest).await;
        }
        Ok(ResolvedGraph::default())
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        if let Some(web) = &self.web {
            return web.install(_graph, project_root, _opts).await;
        }
        match self.language {
            LibLanguage::Rust => {
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
            }
            LibLanguage::Python => {
                exec_tool(
                    project_root,
                    "pip",
                    &["install".to_string(), "-e".to_string(), ".".to_string()],
                )?;
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
        Ok(InstallSummary::default())
    }

    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId> {
        if let Some(web) = &self.web {
            return web.add(project_root, name, range, opts).await;
        }
        if opts.no_save {
            return Ok(placeholder_id(name, range));
        }
        match self.language {
            LibLanguage::Rust => {
                let mut args = vec!["add".to_string()];
                if let Some(r) = range.filter(|r| !r.is_star()) {
                    args.push(format!("{}@{}", name.as_str(), r.as_str()));
                } else {
                    args.push(name.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
                Ok(version_from_manifest(project_root, name, LibLanguage::Rust)
                    .map(|v| PackageId::new(name.clone(), v))
                    .unwrap_or_else(|| placeholder_id(name, range)))
            }
            LibLanguage::Python => {
                check_pip_allowed(project_root, name.as_str())?;
                exec_tool(
                    project_root,
                    "pip",
                    &["install".to_string(), name.as_str().to_string()],
                )?;
                Ok(
                    version_from_manifest(project_root, name, LibLanguage::Python)
                        .map(|v| PackageId::new(name.clone(), v))
                        .unwrap_or_else(|| placeholder_id(name, range)),
                )
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.remove(project_root, name).await;
        }
        match self.language {
            LibLanguage::Rust => {
                exec_tool(
                    project_root,
                    "cargo",
                    &["remove".to_string(), name.as_str().to_string()],
                )?;
            }
            LibLanguage::Python => {
                check_pip_allowed(project_root, name.as_str())?;
                exec_tool(
                    project_root,
                    "pip",
                    &[
                        "uninstall".to_string(),
                        "-y".to_string(),
                        name.as_str().to_string(),
                    ],
                )?;
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
        Ok(())
    }

    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        if let Some(web) = &self.web {
            return web.update(project_root, name).await;
        }
        match self.language {
            LibLanguage::Rust => {
                let mut args = vec!["update".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
            }
            LibLanguage::Python => {
                if let Some(n) = name {
                    check_pip_allowed(project_root, n.as_str())?;
                } else {
                    return Err(mgc_types::MgError::Other(
                        "pip update-all is not allowed — name a package (Q9 allowlist)".to_string(),
                    ));
                }
                let mut args = vec!["install".to_string(), "--upgrade".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "pip", &args)?;
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
        Ok(vec![])
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        if let Some(web) = &self.web {
            return web.list(project_root).await;
        }
        let manifest = self.parse_manifest(project_root).await?;
        let installed: std::collections::HashMap<String, String> = match self.language {
            LibLanguage::Rust => cargo_lock_versions(project_root).into_iter().collect(),
            LibLanguage::Python => dist_info_versions(project_root).into_iter().collect(),
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        };
        Ok(manifest
            .all_dependencies()
            .map(|dep| {
                let version = installed
                    .get(dep.name.as_str())
                    .and_then(|v| Version::parse(v).ok())
                    .or_else(|| dep.range.satisfying_version());
                InstalledPackage {
                    id: PackageId::new(
                        dep.name.clone(),
                        version.unwrap_or_else(|| Version::new(0, 1, 0)),
                    ),
                    path: PathBuf::new(),
                    integrity: None,
                    is_direct: true,
                    is_dev: dep.dev,
                }
            })
            .collect())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        if let Some(web) = &self.web {
            return web.audit(project_root).await;
        }
        let manifest = self.parse_manifest(project_root).await?;
        let count = manifest.all_dependencies().count();
        Ok(AuditReport::clean(count))
    }

    fn set_dedupe_pref(&self, enabled: bool) {
        if let Some(web) = &self.web {
            web.set_dedupe_pref(enabled);
        }
    }

    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {
        if let Some(web) = &self.web {
            web.set_existing_versions(_versions);
        }
    }
}

impl LibAdapter {
    pub fn language(&self) -> &'static str {
        match self.language {
            LibLanguage::Ts => "ts",
            LibLanguage::Rust => "rust",
            LibLanguage::Python => "python",
        }
    }
}

pub fn adapter_for(
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
) -> Option<LibAdapter> {
    let language = detect_language(root)?;
    Some(LibAdapter::for_language(language, registry_url, token))
}

/// Chain version (ITEM 1): primary + fallbacks (url, token).
pub fn adapter_for_with_chain(
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
    fallbacks: &[(String, Option<String>)],
) -> Option<LibAdapter> {
    let language = detect_language(root)?;
    Some(LibAdapter::for_language_with_chain(
        language,
        registry_url,
        token,
        fallbacks,
    ))
}

/// W6: Generate SBOM from lockfile (lib adapter)
pub fn generate_sbom(lockfile: &Lockfile, options: SbomOptions) -> MgResult<String> {
    let generator = SbomGenerator::new(options);
    generator
        .generate_json(lockfile)
        .map_err(|e| mgc_types::MgError::Other(format!("SBOM generation failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mgc-lib-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_rust_project() {
        let dir = tmp_dir("rust");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir, None, None).unwrap();
        assert_eq!(adapter.language(), "rust");
    }

    #[test]
    fn detect_ts_via_mgc_toml() {
        let dir = tmp_dir("ts");
        std::fs::write(
            dir.join("mgc.toml"),
            "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"ts\"\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir, None, None).unwrap();
        assert_eq!(adapter.language(), "ts");
    }

    #[test]
    fn parse_cargo_dependencies() {
        let dir = tmp_dir("parse");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1\"\nsqlx = { version = \"0.7\" }\n",
        )
        .unwrap();
        let manifest = parse_cargo_manifest(&dir).unwrap();
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.dependencies[0].name.as_str(), "serde");
        assert_eq!(manifest.dependencies[0].range.as_str(), "1");
        assert_eq!(manifest.dependencies[1].name.as_str(), "sqlx");
        assert_eq!(manifest.dependencies[1].range.as_str(), "0.7");
    }

    #[test]
    fn write_cargo_preserves_metadata() {
        let dir = tmp_dir("write");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\n",
        )
        .unwrap();
        let mut manifest = parse_cargo_manifest(&dir).unwrap();
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("serde").unwrap(),
                VersionRange::parse("1").unwrap(),
            ),
            false,
            false,
            false,
        );
        write_cargo_manifest(&dir, &manifest).unwrap();
        let re = parse_cargo_manifest(&dir).unwrap();
        assert_eq!(re.dependencies.len(), 1);
        assert_eq!(re.dependencies[0].name.as_str(), "serde");
        let content = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(content.contains("magicore"), "metadata preserved");
    }

    #[test]
    fn pip_allowlist_empty_fail_closed() {
        let dir = tmp_dir("pip-empty");
        std::fs::write(
            dir.join("mgc.toml"),
            "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\n",
        )
        .unwrap();
        let err = check_pip_allowed(&dir, "requests").unwrap_err();
        assert!(
            err.to_string().contains("pip_allowed_packages"),
            "see how to fix"
        );
    }

    #[test]
    fn cargo_lock_versions_reads_real_versions() {
        let dir = tmp_dir("lock");
        std::fs::write(
            dir.join("Cargo.lock"),
            "[[package]]\nname = \"serde\"\nversion = \"1.0.219\"\n\n[[package]]\nname = \"sqlx\"\nversion = \"0.8.6\"\n",
        )
        .unwrap();
        let versions: std::collections::HashMap<String, String> =
            cargo_lock_versions(&dir).into_iter().collect();
        assert_eq!(versions.get("serde").map(String::as_str), Some("1.0.219"));
        assert_eq!(versions.get("sqlx").map(String::as_str), Some("0.8.6"));
    }

    #[test]
    fn dist_info_versions_reads_venv() {
        let dir = tmp_dir("distinfo");
        let meta = dir
            .join(".venv")
            .join("lib")
            .join("python3.11")
            .join("site-packages")
            .join("requests-2.32.3.dist-info");
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::write(
            meta.join("METADATA"),
            "Metadata-Version: 2.1\nName: requests\nVersion: 2.32.3\nSummary: HTTP for Humans\n",
        )
        .unwrap();
        let versions: std::collections::HashMap<String, String> =
            dist_info_versions(&dir).into_iter().collect();
        assert_eq!(versions.get("requests").map(String::as_str), Some("2.32.3"));
    }

    #[test]
    fn pip_allowlist_allows_listed() {
        let dir = tmp_dir("pip-ok");
        std::fs::write(
            dir.join("mgc.toml"),
            "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\npip_allowed_packages = [\"requests\", \"numpy\"]\n",
        )
        .unwrap();
        check_pip_allowed(&dir, "requests").unwrap();
        let err = check_pip_allowed(&dir, "flask").unwrap_err();
        assert!(err.to_string().contains("flask"));
    }

    #[tokio::test]
    async fn update_all_python_fail_closed() {
        let dir = tmp_dir("upd-all");
        std::fs::write(
            dir.join("mgc.toml"),
            "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\npip_allowed_packages = [\"requests\"]\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir, None, None).unwrap();
        let err = adapter.update(&dir, None).await.unwrap_err();
        assert!(
            err.to_string().contains("update-all"),
            "pip update-all is blocked fail-closed"
        );
    }

    #[tokio::test]
    async fn ts_delegate_ignores_workspace_protocol_in_monorepo() {
        let dir = tmp_dir("lib-ws");
        std::fs::write(
            dir.join("package.json"),
            serde_json::json!({
                "name": "frontend",
                "version": "0.1.0",
                "dependencies": {
                    "@core/shared": "workspace:*",
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();
        let adapter = adapter_for(&dir, None, None).unwrap();
        assert_eq!(adapter.language(), "ts");
        let manifest = adapter.parse_manifest(&dir).await.unwrap();
        assert!(manifest.find_dep("react").is_some());
        assert!(manifest.find_dep("@core/shared").is_none());
    }
}

#[test]
fn test_generate_sbom_lib() {
    use mgc_lockfile::{LockfileMetadata, Package};

    let lockfile = Lockfile {
        version: "2".to_string(),
        metadata: LockfileMetadata {
            generated_at: "2026-08-21T00:00:00Z".to_string(),
            generator: "mgc/0.4.0".to_string(),
            lockfile_hash: "abc123".to_string(),
            signer: None,
        },
        packages: vec![Package {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            resolved: "https://example.com/test.tgz".to_string(),
            integrity: "blake3:test123".to_string(),
            dependencies: vec![],
        }],
    };

    let json = generate_sbom(&lockfile, SbomOptions::default()).unwrap();
    assert!(json.contains("CycloneDX"));
    assert!(json.contains("test-pkg"));
    assert!(json.contains("1.0.0"));
}
