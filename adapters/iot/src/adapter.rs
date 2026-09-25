//! PackageAdapter implementation for IoT cores.
//! Điều phối Cargo/PlatformIO/Zephyr riêng khỏi detect và helper tooling.
//!
//! Dependency operations fail closed until a MagiCore-owned ecosystem
//! engine exists. Device flashing remains an explicit hardware-toolchain
//! boundary and is not presented as package management.

use crate::framework::{IotFramework, detect_framework, manifest_is_iot, target_from_manifest};
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
use std::path::Path;

pub struct IotAdapter {
    framework: IotFramework,
}

impl IotAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_framework`/`manifest_is_iot` — real.
    /// - ScaffoldProvider: src/scaffold (esp32-rust/platformio/zephyr
    ///   generators) + the `mgc create-iot` CLI lane — real.
    /// - ContentStoreProvider is not claimed: esp32-rust uses the shared
    ///   native Lib/Rust engine in the CLI; PlatformIO/Zephyr are delegated.
    /// - AuditProvider: shared polyglot dispatch — real.
    ///
    /// DependencyResolver/LockfileProvider NOT claimed: resolve is
    /// fail-closed and write_manifest is real only for esp32-rust.
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
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
    fn probe_content_store(&self) -> MgResult<()> {
        Err(mgc_types::capabilities::unsupported_capability(
            "iot",
            "content_store",
            "esp32-rust uses the shared native Lib/Rust engine; PlatformIO/Zephyr remain toolchain-owned",
        ))
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Err(mgc_types::capabilities::unsupported_capability(
            "iot",
            "install",
            "the IoT adapter has no MagiCore-owned installer for this ecosystem; dependency installation is unsupported",
        ))
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
                        "{} projects do not have a MagiCore-owned dependency manifest; dependency mutation is unsupported",
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
        // No dependency operation is delegated from this adapter.
        // Adapter này không ủy quyền thao tác dependency ra công cụ ngoài.
        let _ = (project_root, name, range, opts);
        Err(mgc_types::capabilities::unsupported_capability(
            "iot",
            "add",
            "the IoT adapter has no MagiCore-owned resolver/writer for this ecosystem; dependency addition is unsupported",
        ))
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        Err(mgc_types::capabilities::unsupported_capability(
            "iot",
            "remove",
            "the IoT adapter has no MagiCore-owned dependency remover for this ecosystem; dependency removal is unsupported",
        ))
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        Err(mgc_types::capabilities::unsupported_capability(
            "iot",
            "update",
            "the IoT adapter has no MagiCore-owned dependency updater for this ecosystem; dependency updates are unsupported",
        ))
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

    fn manifest_identity(&self) -> Option<mgc_types::ManifestIdentity> {
        // Source-verified manifest per framework (only Esp32Rust is
        // mgc-written; Platformio/Zephyr are toolchain-owned and never
        // reach staging, but their identity still mismatches correctly).
        // (Manifest theo framework.)
        let (language, format, relpath) = match self.framework {
            IotFramework::Esp32Rust => ("esp32-rust", "Cargo.toml", "Cargo.toml"),
            IotFramework::Platformio => ("platformio", "platformio.ini", "platformio.ini"),
            IotFramework::Zephyr => ("zephyr", "west.yml", "west.yml"),
        };
        Some(mgc_types::ManifestIdentity {
            core: "iot".to_string(),
            language: language.to_string(),
            format: format.to_string(),
            relpath: relpath.to_string(),
        })
    }

    fn manifest_owned(&self) -> bool {
        // platformio.ini / west.yml are owned SOLELY by their toolchains
        // (write_manifest fails closed for them — same precedent as
        // game/iot non-manifest engines): booking an add in memory would
        // fake a mutation the tool never sees. Only the esp32-rust
        // Cargo.toml is mgc-written.
        // (platformio.ini/west.yml do toolchain sở hữu DUY NHẤT.)
        matches!(self.framework, IotFramework::Esp32Rust)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        match self.framework {
            IotFramework::Esp32Rust => {
                mgc_adapter_base::cargo_manifest::parse_manifest(project_root, Ecosystem::Iot)
            }
            IotFramework::Platformio => parse_platformio_manifest(project_root),
            IotFramework::Zephyr => {
                let name = project_root
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "firmware".to_string());
                Ok(Manifest::new(&name, Ecosystem::Iot))
            }
        }
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let _ = project_root;
        Err(mgc_types::MgError::Unsupported {
            core: "iot",
            capability: "list",
            guidance: format!(
                "{} dependency declarations are not verified installed-state; the IoT adapter has no installed package inventory",
                self.framework.as_str()
            ),
        })
    }
}

/// Parse `lib_deps` from `platformio.ini` `[env:*]` sections (read-only —
/// pio owns the file). Entries are `owner/name[@req]`, comma- or
/// newline-separated, continuations indented. Anything mgc cannot model
/// (URLs, `file://`, `symlink://`) fails CLOSED naming the line — a
/// skipped dep would fake an empty graph (the old parser returned NO
/// deps at all, so `list` lied and the add re-read check could never
/// pass).
/// (Đọc `lib_deps` từ platformio.ini (chỉ đọc); dòng không mô hình được
/// thì lỗi rõ, không bỏ qua âm thầm.)
fn parse_platformio_manifest(project_root: &Path) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(project_root.join("platformio.ini"))
        .map_err(|e| mgc_types::MgError::Other(format!("read platformio.ini: {e}")))?;
    let name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "firmware".to_string());
    let mut manifest = Manifest::new(&name, Ecosystem::Iot);
    let flush = |buf: &mut String, manifest: &mut Manifest| -> MgResult<()> {
        for raw in buf.split([',', '\n']) {
            let spec = raw.trim();
            if spec.is_empty() || spec.starts_with([';', '#']) {
                continue;
            }
            if spec.contains("://") || spec.starts_with("git@") || spec.starts_with("file:") {
                return Err(mgc_types::MgError::Other(format!(
                    "platformio.ini lib_deps entry '{spec}' is not modelable (URL/file/symlink dep) — manage it with pio directly; mgc refuses to fake the graph"
                )));
            }
            let dep = mgc_types::DependencySpec::parse(spec).map_err(|e| {
                mgc_types::MgError::Other(format!("parse platformio.ini lib_deps '{spec}': {e}"))
            })?;
            manifest.add_dep(dep, false, false, false);
        }
        buf.clear();
        Ok(())
    };
    let mut in_env = false;
    let mut collecting = false;
    let mut buf = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            flush(&mut buf, &mut manifest)?;
            collecting = false;
            in_env = trimmed.starts_with("[env:");
            continue;
        }
        if !in_env {
            continue;
        }
        if collecting && (line.starts_with([' ', '\t']) || trimmed.is_empty()) {
            if !trimmed.is_empty() {
                buf.push('\n');
                buf.push_str(trimmed);
            }
            continue;
        }
        if collecting {
            flush(&mut buf, &mut manifest)?;
            collecting = false;
        }
        if let Some((key, value)) = trimmed.split_once('=')
            && key.trim() == "lib_deps"
        {
            buf.push_str(value.trim());
            collecting = true;
        }
    }
    flush(&mut buf, &mut manifest)?;
    Ok(manifest)
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
