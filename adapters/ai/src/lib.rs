#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mg-ai-adapter — AI ecosystem adapter (MegaGate)
//! (Q11/Q20: model pull → CAS store; dev chạy main qua python3. Không resolver
//!  riêng — deps qua pip (allowlist §5.1), mg không quản lý virtualenv)

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
pub enum AiFramework {
    PythonAgent,
    McpServer,
}

impl AiFramework {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiFramework::PythonAgent => "python-agent",
            AiFramework::McpServer => "mcp-server",
        }
    }

    /// Entry script chạy khi `mg dev` — theo scaffold processor.
    pub fn entry_script(&self) -> &'static str {
        match self {
            AiFramework::PythonAgent => "src/agent.py",
            AiFramework::McpServer => "server.py",
        }
    }
}

pub struct AiAdapter {
    pub framework: AiFramework,
}

/// Detect ai project — mg.toml ecosystem=ai (ưu tiên) hoặc pyproject [tool.megagate].
pub fn detect_framework(root: &Path) -> Option<AiFramework> {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("ai")
                .and_then(|c| c.get("framework"))
                .and_then(|p| p.as_str())
            {
                if let Some(fw) = framework_from_str(p) {
                    return Some(fw);
                }
            }
        }
    }
    if let Ok(content) = std::fs::read_to_string(root.join("pyproject.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("tool")
                .and_then(|t| t.get("megagate"))
                .and_then(|m| m.get("framework"))
                .and_then(|p| p.as_str())
            {
                if let Some(fw) = framework_from_str(p) {
                    return Some(fw);
                }
            }
        }
    }
    None
}

fn framework_from_str(s: &str) -> Option<AiFramework> {
    match s {
        "python-agent" => Some(AiFramework::PythonAgent),
        "mcp-server" => Some(AiFramework::McpServer),
        _ => None,
    }
}

pub fn adapter_for(root: &Path) -> Option<AiAdapter> {
    let framework = detect_framework(root)?;
    Some(AiAdapter { framework })
}

fn no_package_manager() -> MgResult<()> {
    Err(mg_types::MgError::Other(
        "ai deps flow through pip (allowlist) — run `pip install -r requirements.txt` manually; mg does not manage virtualenvs".to_string(),
    ))
}

#[async_trait]
impl PackageAdapter for AiAdapter {
    fn name(&self) -> &str {
        "ai"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Ai
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        detect_framework(project_root).is_some()
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "ai".to_string());
        Ok(Manifest::new(&name, Ecosystem::Ai))
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
        no_package_manager()?;
        unreachable!()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-ai-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    #[test]
    fn detect_python_agent_via_pyproject() {
        let dir = tmp_dir("pa");
        std::fs::write(
            dir.join("pyproject.toml"),
            "[tool.megagate]\nframework = \"python-agent\"\n",
        )
        .unwrap();
        assert_eq!(detect_framework(&dir), Some(AiFramework::PythonAgent));
    }

    #[test]
    fn detect_mcp_server_via_pyproject() {
        let dir = tmp_dir("mcp");
        std::fs::write(
            dir.join("pyproject.toml"),
            "[tool.megagate]\nframework = \"mcp-server\"\n",
        )
        .unwrap();
        assert_eq!(detect_framework(&dir), Some(AiFramework::McpServer));
    }

    #[test]
    fn detect_via_mg_toml_framework() {
        let dir = tmp_dir("cfg");
        std::fs::write(dir.join("mg.toml"), "[ai]\nframework = \"mcp-server\"\n").unwrap();
        assert_eq!(detect_framework(&dir), Some(AiFramework::McpServer));
    }

    #[test]
    fn entry_script_matches_scaffold() {
        assert_eq!(AiFramework::PythonAgent.entry_script(), "src/agent.py");
        assert_eq!(AiFramework::McpServer.entry_script(), "server.py");
    }

    #[test]
    fn no_marker_detects_nothing() {
        let dir = tmp_dir("empty");
        assert!(detect_framework(&dir).is_none());
    }
}
