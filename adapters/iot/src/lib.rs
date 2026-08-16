//! mg-iot-adapter — IoT ecosystem adapter (MegaGate)
//! (esp32-rust → orchestrate cargo Q10; platformio/zephyr → exec passthrough; board registry tĩnh P1)

use async_trait::async_trait;
use mg_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mg_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IotFramework {
    Esp32Rust,
    Platformio,
    Zephyr,
}

impl IotFramework {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "esp32-rust" => Some(Self::Esp32Rust),
            "platformio" => Some(Self::Platformio),
            "zephyr" | "zephyr-arm" => Some(Self::Zephyr),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Esp32Rust => "esp32-rust",
            Self::Platformio => "platformio",
            Self::Zephyr => "zephyr",
        }
    }
}

/// ponytail: board registry tĩnh P1 (04 §3) — add board vào đây; P2 chuyển assets/boards/*.json
pub const KNOWN_BOARDS: &[(&str, &str, &str)] = &[
    ("esp32", "esp32", "xtensa-esp32-none-elf"),
    ("esp32c3", "esp32c3", "riscv32imac-unknown-none-elf"),
    ("esp32s3", "esp32s3", "xtensa-esp32s3-none-elf"),
    ("esp32dev", "esp32dev", "riscv32imac-unknown-none-elf"),
    ("nodemcu-32s", "esp32", "xtensa-esp32-none-elf"),
    ("nrf52dk_nrf52832", "nrf52", "thumbv7em-none-eabihf"),
    ("stm32f4_disc", "stm32", "thumbv7em-none-eabihf"),
];

pub fn known_boards() -> Vec<(String, String, String)> {
    KNOWN_BOARDS
        .iter()
        .map(|(id, chip, target)| (id.to_string(), chip.to_string(), target.to_string()))
        .collect()
}

/// Board id → rust target triple (registry KNOWN_BOARDS; None nếu chưa biết).
pub fn board_target(board: &str) -> Option<String> {
    KNOWN_BOARDS
        .iter()
        .find(|(id, _, _)| *id == board)
        .map(|(_, _, target)| target.to_string())
}

pub struct IotAdapter {
    framework: IotFramework,
}

fn detect_framework(root: &Path) -> Option<IotFramework> {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco != "iot" {
                    return None;
                }
            }
            if let Some(fw) = v
                .get("iot")
                .and_then(|i| i.get("framework"))
                .and_then(|f| f.as_str())
            {
                if let Some(framework) = IotFramework::from_str(fw) {
                    return Some(framework);
                }
            }
        }
    }
    if root.join("platformio.ini").exists() {
        return Some(IotFramework::Platformio);
    }
    if root.join("west.yml").exists() {
        return Some(IotFramework::Zephyr);
    }
    if root.join("Cargo.toml").exists() {
        return Some(IotFramework::Esp32Rust);
    }
    None
}

fn manifest_is_iot(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "iot" {
                    return true;
                }
            }
            if v.get("iot").is_some() {
                return true;
            }
        }
    }
    root.join("platformio.ini").exists()
        || root.join("west.yml").exists()
        || root.join("Cargo.toml").exists()
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

fn placeholder_id(name: &PackageName, range: Option<&VersionRange>) -> PackageId {
    let version = range
        .and_then(|r| r.satisfying_version())
        .unwrap_or_else(|| Version::new(0, 1, 0));
    PackageId::new(name.clone(), version)
}

fn cargo_dep_version(root: &Path, name: &PackageName) -> Option<Version> {
    let manifest = mg_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Iot).ok()?;
    manifest
        .find_dep(name.as_str())
        .and_then(|d| d.range.satisfying_version())
}

fn target_from_manifest(root: &Path) -> Option<String> {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(target) = v
                .get("iot")
                .and_then(|i| i.get("target"))
                .and_then(|t| t.as_str())
            {
                return Some(target.to_string());
            }
        }
    }
    None
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
                mg_adapter_base::cargo_manifest::parse_manifest(project_root, Ecosystem::Iot)
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
                mg_adapter_base::cargo_manifest::write_manifest(project_root, manifest)
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
            IotFramework::Esp32Rust => {
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
                Ok(InstallSummary::default())
            }
            IotFramework::Platformio => {
                exec_tool(
                    project_root,
                    "pio",
                    &["pkg".to_string(), "install".to_string()],
                )?;
                Ok(InstallSummary::default())
            }
            IotFramework::Zephyr => {
                exec_tool(project_root, "west", &["update".to_string()])?;
                Ok(InstallSummary::default())
            }
        }
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
            IotFramework::Zephyr => Err(mg_types::MgError::Other(
                "zephyr deps quản lý qua west.yml (passthrough west update) — mg add zephyr chưa hỗ trợ P1 (04 §4)".to_string(),
            )),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        match self.framework {
            IotFramework::Esp32Rust => {
                exec_tool(
                    project_root,
                    "cargo",
                    &["remove".to_string(), name.as_str().to_string()],
                )?;
                Ok(())
            }
            IotFramework::Platformio => {
                exec_tool(
                    project_root,
                    "pio",
                    &[
                        "pkg".to_string(),
                        "uninstall".to_string(),
                        name.as_str().to_string(),
                    ],
                )?;
                Ok(())
            }
            IotFramework::Zephyr => Err(mg_types::MgError::Other(
                "zephyr deps quản lý qua west.yml".to_string(),
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
                Ok(vec![])
            }
            IotFramework::Platformio => {
                let mut args = vec!["pkg".to_string(), "update".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "pio", &args)?;
                Ok(vec![])
            }
            IotFramework::Zephyr => {
                exec_tool(project_root, "west", &["update".to_string()])?;
                Ok(vec![])
            }
        }
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
        let count = manifest.all_dependencies().count();
        Ok(AuditReport::clean(count))
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
        if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-iot-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_esp32_rust_via_mg_toml() {
        let dir = tmp_dir("esp32");
        std::fs::write(
            dir.join("mg.toml"),
            "ecosystem = \"iot\"\n\n[iot]\nframework = \"esp32-rust\"\nboard = \"esp32c3\"\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.framework(), "esp32-rust");
        assert_eq!(adapter.board(&dir).as_deref(), Some("esp32c3"));
    }

    #[test]
    fn detect_platformio_via_ini() {
        let dir = tmp_dir("pio");
        std::fs::write(dir.join("platformio.ini"), "[env:esp32dev]\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.framework(), "platformio");
    }

    #[test]
    fn detect_zephyr_via_west_yml() {
        let dir = tmp_dir("zephyr");
        std::fs::write(dir.join("west.yml"), "manifest:\n  version: 0.13\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.framework(), "zephyr");
    }

    #[test]
    fn board_registry_has_known_targets() {
        let boards = known_boards();
        assert!(boards
            .iter()
            .any(|(id, _, t)| id == "esp32c3" && t == "riscv32imac-unknown-none-elf"));
        assert!(boards.iter().any(|(id, _, _)| id == "nrf52dk_nrf52832"));
    }

    #[test]
    fn board_target_maps_known_boards() {
        assert_eq!(
            board_target("esp32c3").as_deref(),
            Some("riscv32imac-unknown-none-elf")
        );
        assert_eq!(
            board_target("esp32s3").as_deref(),
            Some("xtensa-esp32s3-none-elf")
        );
        assert_eq!(
            board_target("nrf52dk_nrf52832").as_deref(),
            Some("thumbv7em-none-eabihf")
        );
        assert!(board_target("unknown-board").is_none());
    }
}
