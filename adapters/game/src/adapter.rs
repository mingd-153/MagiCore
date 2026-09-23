//! PackageAdapter implementation for game cores.
//! Điều phối Bevy/Godot/Unity/Unreal riêng khỏi detect và helper tooling.
//!
//! Global Gate 1 (2026-09-16): `resolve`/`fetch` overrides are GONE — the
//! fail-closed `MgError::Unsupported` defaults answer now (game engines
//! own their dependency graphs). The Bevy toolchain-delegating
//! add/remove/update stay real (the CLI per-core lane calls `adapter.add`)
//! while the capability is NOT claimed because `resolve` is fail-closed.
//! Global Gate 1: override resolve/fetch đã BỊ XÓA — default
//! `MgError::Unsupported` fail-closed trả lời (engine game tự sở hữu
//! dependency graph). add/remove/update ủy quyền toolchain Bevy giữ
//! nguyên (lane CLI per-core gọi `adapter.add`) nhưng capability KHÔNG
//! được claim vì `resolve` fail-closed.

use crate::engine::{GameEngine, detect_engine, manifest_is_game};
use crate::tooling::{bevy_dep_version, exec_tool, placeholder_id};
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

pub struct GameAdapter {
    engine: GameEngine,
}

impl GameAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_engine`/`manifest_is_game` — real.
    /// - ScaffoldProvider: `mgc create-game <engine>` + src/scaffold — real.
    /// - ContentStoreProvider: install is REAL per engine (Bevy →
    ///   `cargo fetch` via exec_tool; Godot/Unreal/Unity fail closed
    ///   inside install) — claimed.
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
        Capability::ContentStoreProvider,
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
    /// Evidence: install is real per engine — Bevy runs `cargo fetch`
    /// (exec_tool); other engines fail closed inside install.
    /// Dẫn chứng: install thật theo engine — Bevy chạy `cargo fetch`
    /// (exec_tool); engine khác fail-closed bên trong install.
    fn probe_content_store(&self) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        match self.engine {
            GameEngine::Bevy => exec_tool(project_root, "cargo", &["fetch".to_string()])?,
            GameEngine::Godot | GameEngine::Unreal => {
                // Fail closed: no real dependency fetch exists for these
                // engines yet — a silent no-op would fake capability.
                // Fail-closed: engine này chưa có fetch thật — no-op im
                // lặng đồng nghĩa giả capability.
                return Err(mgc_types::MgError::Unsupported {
                    core: "game",
                    capability: "install",
                    guidance: match self.engine {
                        GameEngine::Godot => "Godot projects have no dependency install step; \
                                             open the project in the Godot editor"
                            .to_string(),
                        _ => "Unreal dependency install requires the Epic Launcher \
                              (not automated); open the project in Unreal Editor"
                            .to_string(),
                    },
                });
            }
            GameEngine::Unity => {
                return Err(mgc_types::MgError::Other(
                    "unity install via UPM CLI (Read-and-Verify) is P2 — awaiting spike (03 §7 Q1)"
                        .to_string(),
                ));
            }
        }
        // DELEGATED: install is owned by the toolchain (Bevy → `cargo
        // fetch` ran for real above); mgc does not own this lifecycle.
        // The summary is HONEST — it only counts the packages the
        // manifest graph named. An empty graph yields an empty summary
        // (truthful), never a fabricated package list; cache bytes stay
        // uncounted (cargo owns its cache — Delegated mode).
        // DELEGATED: install thuộc toolchain (Bevy → `cargo fetch` đã chạy
        // thật bên trên); mgc KHÔNG sở hữu lifecycle này. Summary TRUNG
        // THỰC — chỉ đếm package mà graph manifest nêu. Graph rỗng →
        // summary rỗng (trung thực), không bao giờ bịa danh sách package;
        // byte cache không đếm (cargo giữ cache của nó — chế độ Delegated).
        Ok(InstallSummary {
            added: graph.packages.iter().map(|p| p.id.clone()).collect(),
            cache_mode: mgc_types::adapter::InstallCacheMode::Delegated,
            ..Default::default()
        })
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
                        "{} projects do not use mgc-written manifests; regenerate via \
                         `mgc create-game` or edit the project file directly",
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
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId> {
        // DELEGATED: the bevy lane runs `cargo add` + `cargo fetch` for
        // real — mgc orchestrates only; other engines fail closed below.
        // (DELEGATED: lane bevy chạy `cargo add` + `cargo fetch` thật —
        // mgc chỉ điều phối; engine khác fail-closed bên dưới.)
        if opts.no_save {
            return Ok(placeholder_id(name, range));
        }
        match self.engine {
            GameEngine::Bevy => {
                let mut args = vec!["add".to_string()];
                if let Some(r) = range.filter(|r| !r.is_star()) {
                    args.push(format!("{}@{}", name.as_str(), r.as_str()));
                } else {
                    args.push(name.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
                Ok(bevy_dep_version(project_root, name)
                    .map(|v| PackageId::new(name.clone(), v))
                    .unwrap_or_else(|| placeholder_id(name, range)))
            }
            GameEngine::Godot | GameEngine::Unreal => Err(mgc_types::MgError::Other(format!(
                "'{}' has no package manager — game assets are managed outside the dependency graph (03 §4)",
                self.engine.as_str()
            ))),
            GameEngine::Unity => Err(mgc_types::MgError::Other(
                "unity add via UPM CLI (Read-and-Verify) is P2 — awaiting spike (03 §7 Q1)"
                    .to_string(),
            )),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        // DELEGATED: the bevy lane runs `cargo remove` for real — mgc
        // orchestrates only; other engines fail closed below.
        // (DELEGATED: lane bevy chạy `cargo remove` thật — mgc chỉ điều
        // phối; engine khác fail-closed bên dưới.)
        match self.engine {
            GameEngine::Bevy => exec_tool(
                project_root,
                "cargo",
                &["remove".to_string(), name.as_str().to_string()],
            ),
            GameEngine::Godot | GameEngine::Unreal => Err(mgc_types::MgError::Other(format!(
                "'{}' has no package manager",
                self.engine.as_str()
            ))),
            GameEngine::Unity => Err(mgc_types::MgError::Other(
                "unity UPM remove is P2".to_string(),
            )),
        }
    }

    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        // DELEGATED: the bevy lane runs `cargo update` for real — mgc
        // orchestrates only; other engines fail closed below.
        // (DELEGATED: lane bevy chạy `cargo update` thật — mgc chỉ điều
        // phối; engine khác fail-closed bên dưới.)
        match self.engine {
            GameEngine::Bevy => {
                let mut args = vec!["update".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
            }
            GameEngine::Godot | GameEngine::Unreal => {
                return Err(mgc_types::MgError::Other(format!(
                    "'{}' has no package manager",
                    self.engine.as_str()
                )));
            }
            GameEngine::Unity => {
                return Err(mgc_types::MgError::Other(
                    "unity UPM update is P2".to_string(),
                ));
            }
        }
        Ok(vec![])
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
