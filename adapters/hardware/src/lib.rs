//! mg-hardware-adapter — hardware ecosystem adapter (MegaGate)
//! optimizer/bench packages được materialize từ templates/hardware/ bởi CLI
//! (không có native package manager — giống godot/unreal scaffold-only).
//! (ponytail: adapter chỉ detect + manifest; template materialize ở commands/core/hardware.rs)

use async_trait::async_trait;
use mg_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mg_types::{Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version};

use std::path::Path;

pub struct HardwareAdapter;

fn manifest_is_any_mg(root: &Path) -> bool {
    // optimizer/bench là add-on cross-core: materialize được trong MỌI project mg
    // (game/ai/cloud/...), không cần ecosystem = "hardware".
    root.join("mg.toml").is_file()
}

fn placeholder_id(name: &PackageName) -> PackageId {
    PackageId::new(name.clone(), Version::new(0, 1, 0))
}

#[async_trait]
impl PackageAdapter for HardwareAdapter {
    fn name(&self) -> &str {
        "hardware"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Hardware
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_any_mg(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "hardware".to_string());
        Ok(Manifest::new(&name, Ecosystem::Hardware))
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
        _range: Option<&mg_types::VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        Err(mg_types::MgError::Other(
            "hardware packages (optimizer/bench) được materialize bởi `mg add-hardware <pkg>` — không qua registry".to_string(),
        ))
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        Err(mg_types::MgError::Other(
            "hardware packages không qua registry — xóa thủ công folder optimizer/bench"
                .to_string(),
        ))
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        Ok(vec![])
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let mut pkgs = Vec::new();
        for sub in ["optimizer", "bench"] {
            if project_root.join(sub).exists() {
                pkgs.push(InstalledPackage {
                    id: placeholder_id(&PackageName::new(sub)?),
                    path: project_root.join(sub),
                    integrity: None,
                    is_direct: true,
                    is_dev: false,
                });
            }
        }
        Ok(pkgs)
    }

    async fn audit(&self, _project_root: &Path) -> MgResult<AuditReport> {
        Ok(AuditReport::clean(0))
    }
}

pub fn adapter_for(root: &Path) -> Option<HardwareAdapter> {
    manifest_is_any_mg(root).then_some(HardwareAdapter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mg-hardware-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_in_any_mg_project() {
        let dir = tmp_dir("detect");
        std::fs::write(dir.join("mg.toml"), "ecosystem = \"hardware\"\n").unwrap();
        assert!(adapter_for(&dir).is_some());
        let game_dir = tmp_dir("detect-game");
        std::fs::write(game_dir.join("mg.toml"), "ecosystem = \"game\"\n").unwrap();
        assert!(
            adapter_for(&game_dir).is_some(),
            "add-ons cross-core: có thể add optimizer/bench vào project game"
        );
    }

    #[test]
    fn reject_non_mg_projects() {
        let dir = tmp_dir("reject");
        assert!(adapter_for(&dir).is_none());
    }

    #[tokio::test]
    async fn list_reports_optimizer_and_bench_folders() {
        let dir = tmp_dir("list");
        std::fs::write(dir.join("mg.toml"), "ecosystem = \"hardware\"\n").unwrap();
        std::fs::create_dir_all(dir.join("optimizer")).unwrap();
        let adapter = HardwareAdapter;
        let pkgs = adapter.list(&dir).await.unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].id.name().as_str(), "optimizer");
    }
}
