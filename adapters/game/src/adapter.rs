//! PackageAdapter implementation for game cores.
//! Điều phối Bevy/Godot/Unity/Unreal riêng khỏi detect và helper tooling.

use crate::engine::{detect_engine, manifest_is_game, GameEngine};
use crate::tooling::{bevy_dep_version, exec_tool, placeholder_id};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct GameAdapter {
    engine: GameEngine,
}

#[async_trait]
impl PackageAdapter for GameAdapter {
    fn name(&self) -> &str {
        "game"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Game
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_game(project_root)
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

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        match self.engine {
            GameEngine::Bevy => {
                mgc_adapter_base::cargo_manifest::write_manifest(project_root, manifest)
            }
            GameEngine::Godot | GameEngine::Unity | GameEngine::Unreal => Ok(()),
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
        match self.engine {
            GameEngine::Bevy => exec_tool(project_root, "cargo", &["fetch".to_string()])?,
            GameEngine::Godot | GameEngine::Unreal => {}
            GameEngine::Unity => {
                return Err(mgc_types::MgError::Other(
                    "unity install via UPM CLI (Read-and-Verify) is P2 — awaiting spike (03 §7 Q1)"
                        .to_string(),
                ));
            }
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
                "unity add via UPM CLI (Read-and-Verify) is P2 — awaiting spike (03 §7 Q1)".to_string(),
            )),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
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
        Ok(AuditReport::clean(manifest.all_dependencies().count()))
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
