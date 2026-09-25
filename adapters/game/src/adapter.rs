//! PackageAdapter implementation for game cores.
//! Detects Bevy/Godot/Unity/Unreal without claiming an unimplemented
//! MagiCore-owned dependency engine.
//!
//! Dependency operations fail closed until a MagiCore-owned resolver,
//! lock, fetcher, store, and materializer exist for the selected engine.
//! No compatibility package-manager execution is provided by this adapter.
//! Adapter này không cung cấp đường chạy package manager tương thích.

use crate::engine::{GameEngine, detect_engine, manifest_is_game};
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

pub struct GameAdapter {
    engine: GameEngine,
}

impl GameAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_engine`/`manifest_is_game` — real.
    /// - ScaffoldProvider: `mgc create-game <engine>` + src/scaffold — real.
    /// - ContentStoreProvider is not claimed: the CLI Bevy lane uses the
    ///   native Lib/Rust engine; this adapter's legacy helper delegates.
    /// - AuditProvider: shared-engine polyglot dispatch — real.
    ///
    /// DependencyResolver is NOT claimed (resolve is fail-closed — the
    /// engine toolchain owns the graph), though add/remove/update stay
    /// real for Bevy; LockfileProvider is NOT claimed (Bevy-only
    /// write_manifest is not the full trait surface).
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::AuditProvider,
    ];
}

impl CoreIdent for GameAdapter {
    fn core_id(&self) -> &'static str {
        "game"
    }

    fn name(&self) -> &str {
        "game"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Game
    }
}

impl ProjectDetector for GameAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_game(project_root)
    }
}

impl ScaffoldProvider for GameAdapter {
    /// Evidence: src/scaffold (bevy/godot/unity/unreal generators) + the
    /// `mgc create-game` CLI lane.
    /// Dẫn chứng: src/scaffold (bộ sinh bevy/godot/unity/unreal) + lane
    /// CLI `mgc create-game`.
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl ContentStoreProvider for GameAdapter {
    fn probe_content_store(&self) -> MgResult<()> {
        Err(mgc_types::capabilities::unsupported_capability(
            "game",
            "content_store",
            "Bevy uses the shared native Lib/Rust engine; the GameAdapter does not own a store",
        ))
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Err(mgc_types::capabilities::unsupported_capability(
            "game",
            "install",
            "the GameAdapter has no MagiCore-owned installer for this ecosystem; dependency installation is unsupported",
        ))
    }
}

#[async_trait]
impl LockfileProvider for GameAdapter {
    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        match self.engine {
            GameEngine::Bevy => {
                mgc_adapter_base::cargo_manifest::write_manifest(project_root, manifest)
            }
            GameEngine::Godot | GameEngine::Unity | GameEngine::Unreal => {
                // Fail-closed (hardware precedent): the old silent Ok no-op
                // faked a manifest write that never happened.
                // Fail-closed (tiền lệ hardware): Ok no-op cũ giả một lần
                // ghi manifest không bao giờ xảy ra.
                Err(mgc_types::MgError::Unsupported {
                    core: "game",
                    capability: "write_manifest",
                    guidance: format!(
                        "{} projects do not have a MagiCore-owned dependency manifest; dependency mutation is unsupported",
                        self.engine.as_str()
                    ),
                })
            }
        }
    }
}

#[async_trait]
impl DependencyResolver for GameAdapter {
    async fn add(
        &self,
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        Err(mgc_types::capabilities::unsupported_capability(
            "game",
            "add",
            "the GameAdapter has no MagiCore-owned resolver/writer for this ecosystem; dependency addition is unsupported",
        ))
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        Err(mgc_types::capabilities::unsupported_capability(
            "game",
            "remove",
            "the GameAdapter has no MagiCore-owned dependency remover for this ecosystem; dependency removal is unsupported",
        ))
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        Err(mgc_types::capabilities::unsupported_capability(
            "game",
            "update",
            "the GameAdapter has no MagiCore-owned dependency updater for this ecosystem; dependency updates are unsupported",
        ))
    }
}

#[async_trait]
impl AuditProvider for GameAdapter {
    /// Evidence: shared-engine polyglot dispatch (real cargo-audit for
    /// Bevy Cargo.toml; honest UnsupportedEcosystem label otherwise).
    /// Dẫn chứng: dispatch polyglot qua engine chung (cargo-audit thật
    /// cho Cargo.toml Bevy; nhãn UnsupportedEcosystem trung thực còn lại).
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        // Shared-engine polyglot dispatch (P2 2026-09-09): scan every
        // manifest the shared scanners understand (Bevy games ship
        // Cargo.toml → real cargo-audit); engines without any shared
        // manifest keep the honest UnsupportedEcosystem label.
        // Dispatch polyglot qua engine chung: quét mọi manifest mà
        // scanner chung hiểu (game Bevy có Cargo.toml → cargo-audit
        // thật); engine không còn manifest chung nào thì giữ nhãn
        // UnsupportedEcosystem trung thực.
        let manifest = self.parse_manifest(project_root).await?;
        mgc_audit::audit_polyglot(
            project_root,
            format!(
                "game ({} dependencies not scanned — no scanner implemented yet)",
                manifest.all_dependencies().count()
            ),
        )
        .await
    }
}

#[async_trait]
impl PackageAdapter for GameAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    fn manifest_identity(&self) -> Option<mgc_types::ManifestIdentity> {
        // Source-verified manifest per engine (engine.rs detection).
        // (Manifest theo engine — đúng file detect đọc.)
        let (language, format, relpath) = match self.engine {
            GameEngine::Bevy => ("bevy", "Cargo.toml", "Cargo.toml"),
            GameEngine::Godot => ("godot", "project.godot", "project.godot"),
            GameEngine::Unity => ("unity", "manifest.json", "Packages/manifest.json"),
            GameEngine::Unreal => ("unreal", "*.uproject", "*.uproject"),
        };
        Some(mgc_types::ManifestIdentity {
            core: "game".to_string(),
            language: language.to_string(),
            format: format.to_string(),
            relpath: relpath.to_string(),
        })
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        match self.engine {
            GameEngine::Bevy => {
                mgc_adapter_base::cargo_manifest::parse_manifest(project_root, Ecosystem::Game)
            }
            GameEngine::Godot | GameEngine::Unity | GameEngine::Unreal => {
                let name = project_root
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "game".to_string());
                Ok(Manifest::new(&name, Ecosystem::Game))
            }
        }
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let _ = project_root;
        Err(mgc_types::MgError::Unsupported {
            core: "game",
            capability: "list",
            guidance: format!(
                "{} dependency declarations are not verified installed-state; the game adapter has no installed package inventory",
                self.engine.as_str()
            ),
        })
    }
}

impl GameAdapter {
    pub fn detect(root: &Path) -> Option<Self> {
        let engine = detect_engine(root)?;
        Some(Self { engine })
    }

    pub fn engine(&self) -> &'static str {
        self.engine.as_str()
    }
}

pub fn adapter_for(root: &Path) -> Option<GameAdapter> {
    let engine = detect_engine(root)?;
    Some(GameAdapter { engine })
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::ArtifactFetcher for GameAdapter {}
impl mgc_types::capabilities::LifecycleRunner for GameAdapter {}
impl mgc_types::capabilities::OptimizerProvider for GameAdapter {}
impl mgc_types::capabilities::Materializer for GameAdapter {}
impl mgc_types::capabilities::SimulatorProvider for GameAdapter {}
impl mgc_types::capabilities::DeviceProvider for GameAdapter {}
impl mgc_types::capabilities::DeployProvider for GameAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for GameAdapter {}
