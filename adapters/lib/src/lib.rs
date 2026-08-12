//! mg-lib-adapter — library ecosystem adapter (MegaGate)
//! (ts → delegate WebAdapter npm-format; rust → orchestrate cargo Q10; python → pip passthrough)
//! (ponytail: rust/python version resolution = placeholder 0.1.0 khi dry; add khi save chạy tool native)

use async_trait::async_trait;
use mg_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mg_types::{
    DependencySpec, Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version,
    VersionRange,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibLanguage {
    Ts,
    Rust,
    Python,
}

pub struct LibAdapter {
    language: LibLanguage,
    web: Option<mg_web_adapter::WebAdapter>,
}

fn detect_language(root: &Path) -> Option<LibLanguage> {
    let mg_toml = root.join("mg.toml");
    if let Ok(content) = std::fs::read_to_string(&mg_toml) {
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
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
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
    let probes: [(&Path, fn(&Path) -> Option<String>); 3] = [
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
    v.get("megagate")
        .and_then(|m| m.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_cargo_toml(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("megagate"))
        .and_then(|mg| mg.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_pyproject(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("tool")
        .and_then(|t| t.get("megagate"))
        .and_then(|mg| mg.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run(cmd, args, &opts).map_err(|e| mg_types::MgError::Other(e.to_string()))?;
    Ok(())
}

impl LibAdapter {
    fn for_language(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
    ) -> Self {
        let web = if language == LibLanguage::Ts {
            Some(match (registry_url, token) {
                (Some(url), token) => {
                    mg_web_adapter::WebAdapter::with_registry_and_token(url, token)
                }
                (None, _) => mg_web_adapter::WebAdapter::new(),
            })
        } else {
            None
        };
        Self { language, web }
    }
}

fn parse_cargo_manifest(root: &Path) -> MgResult<Manifest> {
    mg_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Lib)
}

fn write_cargo_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    mg_adapter_base::cargo_manifest::write_manifest(root, manifest)
}

fn parse_pyproject_manifest(root: &Path) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(root.join("pyproject.toml"))
        .map_err(|e| mg_types::MgError::Other(format!("read pyproject.toml: {e}")))?;
    let v: toml::Value = toml::from_str(&content)
        .map_err(|e| mg_types::MgError::Other(format!("parse pyproject.toml: {e}")))?;
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
        .map_err(|e| mg_types::MgError::Other(format!("read pyproject.toml: {e}")))?;
    let mut v: toml::Value = toml::from_str(&content)
        .map_err(|e| mg_types::MgError::Other(format!("parse pyproject.toml: {e}")))?;
    let project = v
        .as_table_mut()
        .and_then(|t| t.get_mut("project"))
        .and_then(|p| p.as_table_mut())
        .ok_or_else(|| mg_types::MgError::Other("pyproject.toml missing [project]".to_string()))?;

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
        toml::to_string_pretty(&v).map_err(|e| mg_types::MgError::Other(e.to_string()))?,
    )
    .map_err(|e| mg_types::MgError::Other(format!("write pyproject.toml: {e}")))?;
    Ok(())
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
        Ok(manifest
            .all_dependencies()
            .map(|dep| InstalledPackage {
                id: placeholder_id(&dep.name, Some(&dep.range)),
                path: PathBuf::new(),
                integrity: None,
                is_direct: true,
                is_dev: dep.dev,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-lib-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_rust_project() {
        let dir = tmp_dir("rust");
        let mut f = std::fs::File::create(dir.join("Cargo.toml")).unwrap();
        writeln!(f, "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.megagate]\ncore = \"lib\"\n\n[dependencies]\nserde = \"1\"\n").unwrap();
        let adapter = adapter_for(&dir, None, None).unwrap();
        assert_eq!(adapter.language(), "rust");
    }

    #[test]
    fn detect_ts_via_mg_toml() {
        let dir = tmp_dir("ts");
        std::fs::write(
            dir.join("mg.toml"),
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
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.megagate]\ncore = \"lib\"\n\n[dependencies]\n",
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
        assert!(content.contains("megagate"), "metadata preserved");
    }
}
