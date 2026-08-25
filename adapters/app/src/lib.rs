#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-app-adapter — mobile app ecosystem adapter (MagiCore)
//! (Q18: Kotlin/Flutter/Swift exec passthrough. Không resolver riêng —
//!  install qua tool theo language: flutter pub get / gradle / swift package resolve)

use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

// W6: SBOM support
use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomGenerator, SbomOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLanguage {
    Flutter,
    Kotlin,
    Swift,
    ReactNative,
    ObjC,
    Multi,
}

impl AppLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppLanguage::Flutter => "flutter",
            AppLanguage::Kotlin => "kotlin",
            AppLanguage::Swift => "swift",
            AppLanguage::ReactNative => "react-native",
            AppLanguage::ObjC => "objc",
            AppLanguage::Multi => "multi",
        }
    }
}

pub struct AppAdapter {
    pub language: AppLanguage,
}

/// Detect language — ưu tiên mgc.toml `[app] language`, fallback marker files.
pub fn detect_language(root: &Path) -> Option<AppLanguage> {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("app")
                .and_then(|c| c.get("language"))
                .and_then(|p| p.as_str())
            {
                return match p {
                    "flutter" => Some(AppLanguage::Flutter),
                    "kotlin" => Some(AppLanguage::Kotlin),
                    "swift" => Some(AppLanguage::Swift),
                    "multi" => Some(AppLanguage::Multi),
                    _ => None,
                };
            }
        }
    }
    if root.join("pubspec.yaml").exists() {
        return Some(AppLanguage::Flutter);
    }
    if root.join("build.gradle.kts").exists() || root.join("build.gradle").exists() {
        return Some(AppLanguage::Kotlin);
    }
    if root.join("Package.swift").exists() {
        return Some(AppLanguage::Swift);
    }
    // react-native: package.json có dependency react-native (không phải web npm)
    if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
        if content.contains("\"react-native\"") {
            return Some(AppLanguage::ReactNative);
        }
    }
    // objc entry: nằm trong ios/ của project multi — ObjcBridge.m cạnh ObjcBridge.h
    if root.join("ObjcBridge.h").exists() && root.join("ObjcBridge.m").exists() {
        return Some(AppLanguage::ObjC);
    }
    None
}

fn manifest_is_app(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "app" {
                    return true;
                }
            }
            if v.get("app").is_some() {
                return true;
            }
        }
    }
    detect_language(root).is_some()
}

pub fn adapter_for(root: &Path) -> Option<AppAdapter> {
    let language = detect_language(root)?;
    Some(AppAdapter { language })
}

fn no_package_manager() -> MgResult<()> {
    Err(mgc_types::MgError::Other(
        "app dependencies flow through provider tooling — install with `mgc install` (flutter pub get / gradle / swift package resolve)".to_string(),
    ))
}

#[async_trait]
impl PackageAdapter for AppAdapter {
    fn name(&self) -> &str {
        "app"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::App
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_app(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "app".to_string());
        Ok(Manifest::new(&name, Ecosystem::App))
    }

    async fn write_manifest(&self, _project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
        Ok(())
    }

    async fn resolve(&self, _manifest: &Manifest) -> MgResult<ResolvedGraph> {
        Ok(ResolvedGraph::default())
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Ok(InstallSummary::default())
    }

    async fn add(
        &self,
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        no_package_manager()?;
        unreachable!()
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        no_package_manager()
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        no_package_manager()?;
        unreachable!()
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let manifest = self.parse_manifest(project_root).await?;
        Ok(manifest
            .all_dependencies()
            .map(|dep| InstalledPackage {
                id: PackageId::new(
                    dep.name.clone(),
                    dep.range
                        .satisfying_version()
                        .unwrap_or_else(|| Version::new(0, 1, 0)),
                ),
                path: PathBuf::new(),
                integrity: None,
                is_direct: true,
                is_dev: dep.dev,
            })
            .collect())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        let manifest = self.parse_manifest(project_root).await?;
        Ok(AuditReport::clean(manifest.all_dependencies().count()))
    }

    fn set_dedupe_pref(&self, _enabled: bool) {}

    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {}
}

/// W6: Generate SBOM from lockfile (app adapter)
pub fn generate_sbom(lockfile: &Lockfile, options: SbomOptions) -> MgResult<String> {
    let generator = SbomGenerator::new(options);
    generator
        .generate_json(lockfile)
        .map_err(|e| mgc_types::MgError::Other(format!("SBOM generation failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mgc-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    #[test]
    fn detect_flutter_via_pubspec() {
        let dir = tmp_dir("flutter");
        std::fs::write(dir.join("pubspec.yaml"), "name: myapp\n").unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::Flutter));
    }

    #[test]
    fn detect_kotlin_via_gradle() {
        let dir = tmp_dir("kotlin");
        std::fs::write(dir.join("build.gradle.kts"), "plugins {}\n").unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::Kotlin));
    }

    #[test]
    fn detect_swift_via_package_swift() {
        let dir = tmp_dir("swift");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version:5.9\n").unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::Swift));
    }

    #[test]
    fn detect_via_mgc_toml_language() {
        let dir = tmp_dir("cfg");
        std::fs::write(dir.join("mgc.toml"), "[app]\nlanguage = \"kotlin\"\n").unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::Kotlin));
    }

    #[test]
    fn detect_multi_via_mgc_toml() {
        let dir = tmp_dir("multi");
        std::fs::write(
            dir.join("mgc.toml"),
            "[app]\nlanguage = \"multi\"\nplatforms = [\"android\"]\n",
        )
        .unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::Multi));
    }

    #[test]
    fn detect_react_native_via_package_json() {
        let dir = tmp_dir("rn");
        std::fs::write(
            dir.join("package.json"),
            "{\"dependencies\":{\"react-native\":\"0.74.0\"}}\n",
        )
        .unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::ReactNative));
    }

    #[test]
    fn detect_objc_via_bridge_pair() {
        let dir = tmp_dir("objc");
        std::fs::write(dir.join("ObjcBridge.h"), "@interface MGShared\n").unwrap();
        std::fs::write(dir.join("ObjcBridge.m"), "@implementation MGShared\n").unwrap();
        assert_eq!(detect_language(&dir), Some(AppLanguage::ObjC));
    }

    #[test]
    fn no_manifest_detects_nothing() {
        let dir = tmp_dir("empty");
        assert!(detect_language(&dir).is_none());
    }

    #[test]
    fn add_bails_no_package_manager() {
        let dir = tmp_dir("add");
        std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        let name = PackageName::new("foo").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt
            .block_on(adapter.add(&dir, &name, None, AddOptions::default()))
            .is_err());
    }
}

#[test]
fn test_generate_sbom_app() {
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
