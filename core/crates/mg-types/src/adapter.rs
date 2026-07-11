use crate::ecosystem::Ecosystem;
use crate::error::MgResult;
use crate::manifest::Manifest;
use crate::package::{PackageId, PackageName, VersionRange};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AddOptions {
    pub dev: bool,
    pub optional: bool,
    pub peer: bool,
    pub exact: bool,
    pub no_save: bool,
    pub global: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct InstallSummary {
    pub added: Vec<PackageId>,
    pub bytes_from_cache: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledPackage {
    pub id: PackageId,
    pub path: PathBuf,
    pub integrity: Option<String>,
    pub is_direct: bool,
    pub is_dev: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdatedPackage {
    pub name: String,
    pub from_version: String,
    pub to_version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Vulnerability {
    pub package: PackageId,
    pub title: String,
    pub severity: String,
    pub cve: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuditReport {
    pub packages_audited: usize,
    pub vulnerability_count: usize,
    pub vulnerabilities: Vec<Vulnerability>,
}

impl AuditReport {
    pub fn clean(vulnerability_count: usize) -> Self {
        Self {
            packages_audited: 0,
            vulnerability_count,
            vulnerabilities: vec![],
        }
    }

    pub fn is_clean(&self) -> bool {
        self.vulnerabilities.is_empty() && self.vulnerability_count == 0
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ResolvedGraph {
    pub packages: Vec<ResolvedPackage>,
}

impl ResolvedGraph {
    pub fn empty() -> Self {
        Self { packages: vec![] }
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolvedPackage {
    pub id: PackageId,
    pub integrity: String,
    pub tarball_url: String,
    pub deps: Vec<PackageId>,
    pub direct: bool,
    pub dev: bool,
}

#[async_trait]
pub trait PackageAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn ecosystem(&self) -> Ecosystem;
    fn can_handle(&self, project_root: &Path) -> bool;

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest>;
    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()>;
    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph>;
    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()>;
    async fn install(&self, graph: &ResolvedGraph, project_root: &Path)
        -> MgResult<InstallSummary>;
    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId>;
    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()>;
    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>>;
    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>>;
    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport>;
}
