//! mg-cicd-adapter — CI/CD ecosystem adapter (MegaGate)
//! (Q12: CI + deploy đa cloud — GitHub Actions, Cloudflare, AWS, GCP, ArgoCD.
//!  Không có package manager riêng: add/remove/update fail-closed; deploy/exec qua CLI provider)

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
pub enum CicdProvider {
    GithubActions,
    Gitlab,
    CircleCi,
    Cloudflare,
    Aws,
    Gcp,
    Argocd,
}

impl CicdProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            CicdProvider::GithubActions => "github-actions",
            CicdProvider::Gitlab => "gitlab",
            CicdProvider::CircleCi => "circleci",
            CicdProvider::Cloudflare => "cloudflare",
            CicdProvider::Aws => "aws",
            CicdProvider::Gcp => "gcp",
            CicdProvider::Argocd => "argocd",
        }
    }
}

pub struct CicdAdapter {
    pub provider: CicdProvider,
}

/// Detect provider — ưu tiên mg.toml `[cicd] provider`, fallback manifest probe.
pub fn detect_provider(root: &Path) -> Option<CicdProvider> {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("cicd")
                .and_then(|c| c.get("provider"))
                .and_then(|p| p.as_str())
            {
                return match p {
                    "github-actions" => Some(CicdProvider::GithubActions),
                    "gitlab" => Some(CicdProvider::Gitlab),
                    "circleci" => Some(CicdProvider::CircleCi),
                    "cloudflare" => Some(CicdProvider::Cloudflare),
                    "aws" => Some(CicdProvider::Aws),
                    "gcp" => Some(CicdProvider::Gcp),
                    "argocd" => Some(CicdProvider::Argocd),
                    _ => None,
                };
            }
        }
    }
    if root.join("wrangler.toml").exists() {
        return Some(CicdProvider::Cloudflare);
    }
    if root.join("argocd").join("application.yaml").exists() {
        return Some(CicdProvider::Argocd);
    }
    if root.join(".github").join("workflows").exists() {
        return Some(CicdProvider::GithubActions);
    }
    if root.join(".gitlab-ci.yml").exists() {
        return Some(CicdProvider::Gitlab);
    }
    if root.join(".circleci").join("config.yml").exists() {
        return Some(CicdProvider::CircleCi);
    }
    if root.join("main.tf").exists() {
        return Some(CicdProvider::Aws);
    }
    None
}

fn manifest_is_cicd(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "cicd" {
                    return true;
                }
            }
            if v.get("cicd").is_some() {
                return true;
            }
        }
    }
    detect_provider(root).is_some()
}

pub fn adapter_for(root: &Path) -> Option<CicdAdapter> {
    let provider = detect_provider(root)?;
    Some(CicdAdapter { provider })
}

fn no_package_manager() -> MgResult<()> {
    Err(mg_types::MgError::Other(
        "cicd has no package manager — deploy through `mg deploy` (dry-run default)".to_string(),
    ))
}

#[async_trait]
impl PackageAdapter for CicdAdapter {
    fn name(&self) -> &str {
        "cicd"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cicd
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_cicd(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "ci".to_string());
        Ok(Manifest::new(&name, Ecosystem::Cicd))
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

impl CicdAdapter {
    pub fn provider(&self) -> &'static str {
        self.provider.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-cicd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    #[test]
    fn detect_cloudflare_via_wrangler_toml() {
        let dir = tmp_dir("cf");
        std::fs::write(dir.join("wrangler.toml"), "name = \"worker\"\n").unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::Cloudflare));
    }

    #[test]
    fn detect_github_actions_via_workflows_dir() {
        let dir = tmp_dir("gha");
        std::fs::create_dir_all(dir.join(".github").join("workflows")).unwrap();
        std::fs::write(
            dir.join(".github").join("workflows").join("ci.yml"),
            "name: CI\n",
        )
        .unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::GithubActions));
    }

    #[test]
    fn detect_via_mg_toml_provider() {
        let dir = tmp_dir("cfg");
        std::fs::write(dir.join("mg.toml"), "[cicd]\nprovider = \"aws\"\n").unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::Aws));
    }

    #[test]
    fn detect_gitlab_via_mg_toml_and_probe() {
        let dir = tmp_dir("gl");
        std::fs::write(dir.join("mg.toml"), "[cicd]\nprovider = \"gitlab\"\n").unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::Gitlab));
        std::fs::remove_file(dir.join("mg.toml")).unwrap();
        std::fs::write(dir.join(".gitlab-ci.yml"), "stages: [ci]\n").unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::Gitlab));
    }

    #[test]
    fn detect_circleci_via_mg_toml_and_probe() {
        let dir = tmp_dir("cc");
        std::fs::write(dir.join("mg.toml"), "[cicd]\nprovider = \"circleci\"\n").unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::CircleCi));
        std::fs::remove_file(dir.join("mg.toml")).unwrap();
        std::fs::create_dir_all(dir.join(".circleci")).unwrap();
        std::fs::write(dir.join(".circleci").join("config.yml"), "version: 2.1\n").unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::CircleCi));
    }

    #[test]
    fn detect_argocd_via_application_yaml() {
        let dir = tmp_dir("argocd");
        std::fs::create_dir_all(dir.join("argocd")).unwrap();
        std::fs::write(
            dir.join("argocd").join("application.yaml"),
            "kind: Application\n",
        )
        .unwrap();
        assert_eq!(detect_provider(&dir), Some(CicdProvider::Argocd));
    }

    #[test]
    fn no_manifest_detects_nothing() {
        let dir = tmp_dir("empty");
        assert!(detect_provider(&dir).is_none());
    }

    #[test]
    fn add_bails_no_package_manager() {
        let dir = tmp_dir("add");
        std::fs::write(dir.join("wrangler.toml"), "name = \"worker\"\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        let name = PackageName::new("foo").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt
            .block_on(adapter.add(&dir, &name, None, AddOptions::default()))
            .is_err());
    }
}
