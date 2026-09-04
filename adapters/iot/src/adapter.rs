//! PackageAdapter implementation for IoT cores.
//! Điều phối Cargo/PlatformIO/Zephyr riêng khỏi detect và helper tooling.

use crate::framework::{detect_framework, manifest_is_iot, target_from_manifest, IotFramework};
use crate::tooling::{cargo_dep_version, exec_tool, placeholder_id};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct IotAdapter {
    framework: IotFramework,
}

#[async_trait]
impl PackageAdapter for IotAdapter {
    fn name(&self) -> &str {
        "iot"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Iot
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_iot(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        match self.framework {
            IotFramework::Esp32Rust => {
                mgc_adapter_base::cargo_manifest::parse_manifest(project_root, Ecosystem::Iot)
            }
            IotFramework::Platformio | IotFramework::Zephyr => {
                let name = project_root
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "firmware".to_string());
                Ok(Manifest::new(&name, Ecosystem::Iot))
            }
        }
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        match self.framework {
            IotFramework::Esp32Rust => {
                mgc_adapter_base::cargo_manifest::write_manifest(project_root, manifest)
            }
            IotFramework::Platformio | IotFramework::Zephyr => Ok(()),
        }
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
        project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        match self.framework {
            IotFramework::Esp32Rust => exec_tool(project_root, "cargo", &["fetch".to_string()])?,
            IotFramework::Platformio => {
                exec_tool(
                    project_root,
                    "pio",
                    &["pkg".to_string(), "install".to_string()],
                )?;
            }
            IotFramework::Zephyr => exec_tool(project_root, "west", &["update".to_string()])?,
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
        if opts.no_save {
            return Ok(placeholder_id(name, range));
        }
        match self.framework {
            IotFramework::Esp32Rust => {
                let mut args = vec!["add".to_string()];
                if let Some(r) = range.filter(|r| !r.is_star()) {
                    args.push(format!("{}@{}", name.as_str(), r.as_str()));
                } else {
                    args.push(name.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
                Ok(cargo_dep_version(project_root, name)
                    .map(|v| PackageId::new(name.clone(), v))
                    .unwrap_or_else(|| placeholder_id(name, range)))
            }
            IotFramework::Platformio => {
                let mut args = vec!["pkg".to_string(), "install".to_string()];
                if let Some(r) = range.filter(|r| !r.is_star()) {
                    args.push(format!("{}@{}", name.as_str(), r.as_str()));
                } else {
                    args.push(name.as_str().to_string());
                }
                exec_tool(project_root, "pio", &args)?;
                Ok(placeholder_id(name, range))
            }
            IotFramework::Zephyr => Err(mgc_types::MgError::Other(
                "zephyr deps are managed via west.yml (passthrough west update) — mgc add for zephyr is not supported yet, P1 (04 §4)".to_string(),
            )),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        match self.framework {
            IotFramework::Esp32Rust => exec_tool(
                project_root,
                "cargo",
                &["remove".to_string(), name.as_str().to_string()],
            ),
            IotFramework::Platformio => exec_tool(
                project_root,
                "pio",
                &[
                    "pkg".to_string(),
                    "uninstall".to_string(),
                    name.as_str().to_string(),
                ],
            ),
            IotFramework::Zephyr => Err(mgc_types::MgError::Other(
                "zephyr deps are managed via west.yml".to_string(),
            )),
        }
    }

    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        match self.framework {
            IotFramework::Esp32Rust => {
                let mut args = vec!["update".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
            }
            IotFramework::Platformio => {
                let mut args = vec!["pkg".to_string(), "update".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "pio", &args)?;
            }
            IotFramework::Zephyr => exec_tool(project_root, "west", &["update".to_string()])?,
        }
        Ok(vec![])
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
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
        let manifest = self.parse_manifest(project_root).await?;
        // P0.6 FIX: Return unavailable instead of fake clean
        Ok(AuditReport::unavailable(format!(
            "No audit scanner available for IoT core ({} dependencies not scanned)",
            manifest.all_dependencies().count()
        )))
    }
}

impl IotAdapter {
    pub fn detect(root: &Path) -> Option<Self> {
        let framework = detect_framework(root)?;
        Some(Self { framework })
    }

    pub fn framework(&self) -> &'static str {
        self.framework.as_str()
    }

    pub fn board(&self, root: &Path) -> Option<String> {
        if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
            if let Ok(v) = toml::from_str::<toml::Value>(&content) {
                return v
                    .get("iot")
                    .and_then(|i| i.get("board"))
                    .and_then(|b| b.as_str())
                    .map(str::to_string);
            }
        }
        None
    }

    pub fn target(&self, root: &Path) -> Option<String> {
        target_from_manifest(root)
    }
}

pub fn adapter_for(root: &Path) -> Option<IotAdapter> {
    let framework = detect_framework(root)?;
    Some(IotAdapter { framework })
}
