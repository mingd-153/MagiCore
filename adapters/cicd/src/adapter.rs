//! PackageAdapter implementation for CI/CD cores.
//! Điều phối dependency flow fail-closed riêng khỏi provider detection.

use crate::provider::{CicdProvider, detect_provider, manifest_is_cicd};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct CicdAdapter {
    pub provider: CicdProvider,
}

pub fn adapter_for(root: &Path) -> Option<CicdAdapter> {
    let provider = detect_provider(root)?;
    Some(CicdAdapter { provider })
}

fn no_package_manager() -> MgResult<()> {
    Err(mgc_types::MgError::Other(
        "cicd has no package manager — deploy through `mgc deploy` (dry-run default)".to_string(),
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
        // Fail-closed: pipeline files are hand-owned — no-op success would
        // fake a manifest write that never happened.
        // Fail-closed: file pipeline do người quản lý — Ok no-op là giả.
        Err(mgc_types::MgError::Unsupported {
            core: "cicd",
            capability: "write_manifest",
            guidance: "CI/CD pipeline files are maintained by hand; \
                       scaffold via `mgc create-cicd <provider>` instead"
                .to_string(),
        })
    }

    async fn resolve(&self, _manifest: &Manifest) -> MgResult<ResolvedGraph> {
        // CI/CD providers have no registry dependency graph — fail closed.
        // Provider CI/CD không có dependency graph registry — fail-closed.
        Err(mgc_types::MgError::Unsupported {
            core: "cicd",
            capability: "resolve",
            guidance: "CI/CD templates have no dependency graph to resolve".to_string(),
        })
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Err(mgc_types::MgError::Unsupported {
            core: "cicd",
            capability: "fetch",
            guidance: "nothing to fetch — CI/CD templates carry no registry packages".to_string(),
        })
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Err(mgc_types::MgError::Unsupported {
            core: "cicd",
            capability: "install",
            guidance: "CI/CD has no package install; deploy through `mgc deploy` \
                       (dry-run default)"
                .to_string(),
        })
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
        // P2 2026-09-10: cicd owns its TARGET-ecosystem lane — GitHub
        // Actions SHA-pinning/permissions/injection policy — plus the
        // polyglot engine for sibling dependency manifests.
        // Cicd có lane ecosystem ĐÍCH — policy SHA-pinning/permissions/
        // injection của GitHub Actions — cộng engine polyglot cho
        // manifest dependency kề.
        let mut plan = mgc_audit::plan_for_shared_manifests(project_root)?;
        let has_workflows = project_root.join(".github").join("workflows").is_dir();
        if has_workflows {
            let root = project_root.to_path_buf();
            plan.add_step(mgc_audit::ScanStep {
                ecosystem: "cicd/github-actions",
                scanner: "github-actions-policy",
                run: Box::new(move || {
                    let root = root.clone();
                    Box::pin(async move { mgc_audit::scanners::audit_github_actions(&root) })
                }),
            });
        }
        let manifest = self.parse_manifest(project_root).await?;
        let label = format!(
            "cicd ({} dependencies not scanned — no scanner implemented yet)",
            manifest.all_dependencies().count()
        );
        if plan.is_empty() {
            return Ok(mgc_types::adapter::AuditReport::unsupported_ecosystem(
                label,
            ));
        }
        plan.execute().await
    }

    fn set_dedupe_pref(&self, _enabled: bool) {}

    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {}
}

impl CicdAdapter {
    pub fn provider(&self) -> &'static str {
        self.provider.as_str()
    }
}
