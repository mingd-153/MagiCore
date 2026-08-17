//! mg-game-adapter — game ecosystem adapter (MegaGate)
//! (bevy → orchestrate cargo Q10; godot → scaffold-only + mg dev editor; unity → UPM P2; unreal → scaffold-only)
//! (ponytail: unity mg add/install qua UPM CLI Read-and-Verify là P2 — chờ spike 03 §7 Q1)

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
pub enum GameEngine {
    Bevy,
    Godot,
    Unity,
    Unreal,
}

impl GameEngine {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "bevy" => Some(Self::Bevy),
            "godot" => Some(Self::Godot),
            "unity" => Some(Self::Unity),
            "unreal" => Some(Self::Unreal),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Bevy => "bevy",
            Self::Godot => "godot",
            Self::Unity => "unity",
            Self::Unreal => "unreal",
        }
    }
}

pub struct GameAdapter {
    engine: GameEngine,
}

pub fn detect_engine(root: &Path) -> Option<GameEngine> {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco != "game" && v.get("game").is_none() {
                    return None;
                }
            }
            if let Some(engine) = v
                .get("game")
                .and_then(|g| g.get("engine"))
                .and_then(|e| e.as_str())
            {
                return GameEngine::from_str(engine);
            }
        }
    }
    if root.join("project.godot").exists() {
        return Some(GameEngine::Godot);
    }
    if root.join("Packages").join("manifest.json").exists() {
        return Some(GameEngine::Unity);
    }
    if root
        .read_dir()
        .ok()?
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().map_or(false, |x| x == "uproject"))
    {
        return Some(GameEngine::Unreal);
    }
    if root.join("Cargo.toml").exists() {
        return Some(GameEngine::Bevy);
    }
    None
}

fn manifest_is_game(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "game" {
                    return true;
                }
            }
            if v.get("game").is_some() {
                return true;
            }
        }
    }
    if root.join("project.godot").exists()
        || root.join("Packages").join("manifest.json").exists()
        || root.join("Cargo.toml").exists()
        || root.read_dir().ok().is_some_and(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| e.path().extension().map_or(false, |x| x == "uproject"))
        })
    {
        return true;
    }
    false
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

fn bevy_dep_version(root: &Path, name: &PackageName) -> Option<Version> {
    let manifest = mg_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Game).ok()?;
    manifest
        .find_dep(name.as_str())
        .and_then(|d| d.range.satisfying_version())
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
                mg_adapter_base::cargo_manifest::parse_manifest(project_root, Ecosystem::Game)
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
                mg_adapter_base::cargo_manifest::write_manifest(project_root, manifest)
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
            GameEngine::Bevy => {
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
                Ok(InstallSummary::default())
            }
            GameEngine::Godot | GameEngine::Unreal => Ok(InstallSummary::default()),
            GameEngine::Unity => Err(mg_types::MgError::Other(
                "unity install qua UPM CLI (Read-and-Verify) là P2 — chờ spike (03 §7 Q1)"
                    .to_string(),
            )),
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
            GameEngine::Godot | GameEngine::Unreal => Err(mg_types::MgError::Other(format!(
                "'{}' has no package manager — game assets manage ngoài dependency graph (03 §4)",
                self.engine.as_str()
            ))),
            GameEngine::Unity => Err(mg_types::MgError::Other(
                "unity add qua UPM CLI (Read-and-Verify) là P2 — chờ spike (03 §7 Q1)".to_string(),
            )),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        match self.engine {
            GameEngine::Bevy => {
                exec_tool(
                    project_root,
                    "cargo",
                    &["remove".to_string(), name.as_str().to_string()],
                )?;
                Ok(())
            }
            GameEngine::Godot | GameEngine::Unreal => Err(mg_types::MgError::Other(format!(
                "'{}' has no package manager",
                self.engine.as_str()
            ))),
            GameEngine::Unity => Err(mg_types::MgError::Other(
                "unity UPM remove là P2".to_string(),
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
                Ok(vec![])
            }
            GameEngine::Godot | GameEngine::Unreal => Err(mg_types::MgError::Other(format!(
                "'{}' has no package manager",
                self.engine.as_str()
            ))),
            GameEngine::Unity => Err(mg_types::MgError::Other(
                "unity UPM update là P2".to_string(),
            )),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-game-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_bevy_via_mg_toml() {
        let dir = tmp_dir("bevy");
        std::fs::write(
            dir.join("mg.toml"),
            "ecosystem = \"game\"\n\n[game]\nengine = \"bevy\"\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.engine(), "bevy");
    }

    #[test]
    fn detect_godot_via_project_file() {
        let dir = tmp_dir("godot");
        std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.engine(), "godot");
    }

    #[test]
    fn detect_unreal_via_uproject() {
        let dir = tmp_dir("unreal");
        std::fs::write(dir.join("MyGame.uproject"), "{\"FileVersion\": 3}\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.engine(), "unreal");
    }

    #[test]
    fn detect_unity_via_manifest() {
        let dir = tmp_dir("unity");
        std::fs::create_dir_all(dir.join("Packages")).unwrap();
        std::fs::write(
            dir.join("Packages").join("manifest.json"),
            "{\"dependencies\": {}}\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.engine(), "unity");
    }

    #[tokio::test]
    async fn bevy_parse_cargo_manifest() {
        let dir = tmp_dir("bevy-parse");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nbevy = \"0.14\"\n",
        )
        .unwrap();
        let adapter = adapter_for(&dir).unwrap();
        let manifest = adapter.parse_manifest(&dir).await.unwrap();
        assert_eq!(manifest.dependencies[0].name.as_str(), "bevy");
    }
}
