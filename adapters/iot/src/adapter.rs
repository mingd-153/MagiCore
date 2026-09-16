//! PackageAdapter implementation for IoT cores.
//! Điều phối Cargo/PlatformIO/Zephyr riêng khỏi detect và helper tooling.
//!
//! Global Gate 1 (2026-09-16): `resolve`/`fetch` overrides are GONE — the
//! fail-closed defaults answer now (IoT toolchains own their graphs).
//! The toolchain-delegating add/remove/update stay real (the CLI
//! per-core lane calls `adapter.add`) while the DependencyResolver
//! capability is NOT claimed because `resolve` is fail-closed. The
//! silent Ok no-op for Platformio/Zephyr write_manifest is now
//! fail-closed too (hardware precedent).
//! Global Gate 1: override resolve/fetch đã BỊ XÓA — default fail-closed
//! trả lời (toolchain IoT tự sở hữu graph). add/remove/update ủy quyền
//! toolchain giữ nguyên (lane CLI per-core gọi `adapter.add`) nhưng
//! capability DependencyResolver KHÔNG claim vì `resolve` fail-closed.
//! No-op Ok âm thầm của write_manifest Platformio/Zephyr giờ cũng
//! fail-closed (tiền lệ hardware).

use crate::framework::{IotFramework, detect_framework, manifest_is_iot, target_from_manifest};
use crate::tooling::{cargo_dep_version, exec_tool, placeholder_id};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::capabilities::{
    AuditProvider, Capability, ContentStoreProvider, CoreIdent, DependencyResolver,
    LockfileProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct IotAdapter {
    framework: IotFramework,
}

impl IotAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_framework`/`manifest_is_iot` — real.
    /// - ScaffoldProvider: src/scaffold (esp32-rust/platformio/zephyr
    ///   generators) + the `mgc create-iot` CLI lane — real.
    /// - ContentStoreProvider: install is REAL per framework (esp32-rust
    ///   → `cargo fetch`, platformio → `pio pkg install`, zephyr →
    ///   `west update`) — claimed.
    /// - AuditProvider: shared polyglot dispatch — real.
    ///
    /// DependencyResolver/LockfileProvider NOT claimed: resolve is
    /// fail-closed and write_manifest is real only for esp32-rust.
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::ContentStoreProvider,
        Capability::AuditProvider,
    ];
}

impl CoreIdent for IotAdapter {
    fn core_id(&self) -> &'static str {
        "iot"
    }

    fn name(&self) -> &str {
        "iot"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Iot
    }
}

impl ProjectDetector for IotAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_iot(project_root)
    }
}

impl ScaffoldProvider for IotAdapter {
    /// Evidence: src/scaffold generators + the `mgc create-iot` CLI lane.
    /// Dẫn chứng: bộ sinh src/scaffold + lane CLI `mgc create-iot`.
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl ContentStoreProvider for IotAdapter {
    /// Evidence: install is real per framework (cargo fetch / pio pkg
    /// install / west update via exec_tool below).
    /// Dẫn chứng: install thật theo framework (cargo fetch / pio pkg
    /// install / west update qua exec_tool bên dưới).
    fn probe_content_store(&self) -> MgResult<()> {
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
}

#[async_trait]
impl LockfileProvider for IotAdapter {
    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        match self.framework {
            IotFramework::Esp32Rust => {
                mgc_adapter_base::cargo_manifest::write_manifest(project_root, manifest)
            }
            IotFramework::Platformio | IotFramework::Zephyr => {
                // Fail-closed (hardware precedent): the old silent Ok no-op
                // faked a manifest write that never happened.
                // Fail-closed (tiền lệ hardware): Ok no-op cũ giả một lần
                // ghi manifest không bao giờ xảy ra.
                Err(mgc_types::MgError::Unsupported {
                    core: "iot",
                    capability: "write_manifest",
                    guidance: format!(
                        "{} projects do not use mgc-written manifests; regenerate via \
                         `mgc create-iot` or edit platformio.ini/west.yml directly",
                        self.framework.as_str()
                    ),
                })
            }
        }
    }
}

#[async_trait]
impl DependencyResolver for IotAdapter {
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
}

#[async_trait]
impl AuditProvider for IotAdapter {
    /// Evidence: shared polyglot dispatch below (Rust/Python/Go manifests
    /// get real scans; pure-embedded stays honestly unsupported).
    /// Dẫn chứng: dispatch polyglot bên dưới (manifest Rust/Python/Go
    /// được quét thật; thuần embedded giữ unsupported trung thực).
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        // Shared-engine polyglot dispatch (P2): Rust/Python/Go manifests
        // in IoT projects get real scans; pure-embedded manifests stay
        // honestly unsupported.
        // Dispatch polyglot qua engine chung: manifest Rust/Python/Go
        // trong project IoT được quét thật; manifest thuần embedded giữ
        // trung thực unsupported.
        let manifest = self.parse_manifest(project_root).await?;
        mgc_audit::audit_polyglot(
            project_root,
            format!(
                "iot ({} dependencies not scanned — no scanner implemented yet)",
                manifest.all_dependencies().count()
            ),
        )
        .await
    }
}

#[async_trait]
impl PackageAdapter for IotAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
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
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml"))
            && let Ok(v) = toml::from_str::<toml::Value>(&content)
        {
            return v
                .get("iot")
                .and_then(|i| i.get("board"))
                .and_then(|b| b.as_str())
                .map(str::to_string);
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

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::ArtifactFetcher for IotAdapter {}
impl mgc_types::capabilities::LifecycleRunner for IotAdapter {}
impl mgc_types::capabilities::OptimizerProvider for IotAdapter {}
impl mgc_types::capabilities::Materializer for IotAdapter {}
impl mgc_types::capabilities::SimulatorProvider for IotAdapter {}
impl mgc_types::capabilities::DeviceProvider for IotAdapter {}
impl mgc_types::capabilities::DeployProvider for IotAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for IotAdapter {}
