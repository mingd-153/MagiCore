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

/// Options controlling install behaviour.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct InstallOptions {
    /// Skip running lifecycle scripts (preinstall / install / postinstall).
    pub ignore_scripts: bool,
    /// Explicitly allow lifecycle scripts. Defaults to false for secure installs.
    pub allow_scripts: bool,
    /// Use flat hoisting layout instead of strict symlink virtual store.
    pub legacy_flat: bool,
    /// Fail fast if mg.lock is missing or out-of-sync (CI mode).
    pub frozen: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VulnerabilitySeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl VulnerabilitySeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Info => "info",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" => Self::Critical,
            "high" => Self::High,
            "medium" | "moderate" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Info,
        }
    }

    /// Returns true if this severity is at least as severe as `other`.
    pub fn is_at_least(&self, other: &Self) -> bool {
        let rank = |s: &Self| match s {
            Self::Critical => 4,
            Self::High => 3,
            Self::Medium => 2,
            Self::Low => 1,
            Self::Info => 0,
        };
        rank(self) >= rank(other)
    }
}

impl std::fmt::Display for VulnerabilitySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Vulnerability {
    pub package: PackageId,
    pub title: String,
    /// Raw severity string (for display / backward compat)
    pub severity: String,
    pub cve: String,
    /// Structured severity level.
    pub severity_level: VulnerabilitySeverity,
    /// Version range(s) that contain a fix, if known.
    pub patched_versions: Option<String>,
    /// Link to the advisory.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuditReport {
    pub packages_audited: usize,
    pub vulnerability_count: usize,
    pub vulnerabilities: Vec<Vulnerability>,
}

impl AuditReport {
    pub fn clean(packages_audited: usize) -> Self {
        Self {
            packages_audited,
            vulnerability_count: 0,
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

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
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
    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary>;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_report_is_clean_when_no_vulns() {
        let report = AuditReport::clean(0);
        assert!(report.is_clean());
    }

    #[test]
    fn audit_report_clean_tracks_packages_audited_without_vulns() {
        let report = AuditReport::clean(3);
        assert_eq!(report.packages_audited, 3);
        assert_eq!(report.vulnerability_count, 0);
        assert!(report.is_clean());
    }

    #[test]
    fn audit_report_not_clean_with_vulnerabilities_vec() {
        let report = AuditReport {
            packages_audited: 1,
            vulnerability_count: 1,
            vulnerabilities: vec![Vulnerability {
                package: PackageId::parse("test@1.0.0").unwrap(),
                title: "vuln".to_string(),
                severity: "high".to_string(),
                cve: "CVE-123".to_string(),
                severity_level: VulnerabilitySeverity::High,
                patched_versions: None,
                url: None,
            }],
        };
        assert!(!report.is_clean());
    }

    #[test]
    fn audit_report_not_clean_with_nonzero_count() {
        let report = AuditReport {
            packages_audited: 3,
            vulnerability_count: 3,
            vulnerabilities: vec![],
        };
        assert!(!report.is_clean());
    }

    #[test]

    fn resolved_graph_empty_creates_empty() {
        let g = ResolvedGraph::empty();
        assert_eq!(g.len(), 0);
        assert!(g.is_empty());
    }

    #[test]
    fn resolved_graph_default_is_empty() {
        let g = ResolvedGraph::default();
        assert!(g.is_empty());
    }

    #[test]
    fn resolved_graph_with_packages_not_empty() {
        let g = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: PackageId::parse("foo@1.0.0").unwrap(),
                integrity: "sha1-xxx".to_string(),
                tarball_url: "https://example.com/pkg.tgz".to_string(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        assert_eq!(g.len(), 1);
        assert!(!g.is_empty());
    }

    #[test]
    fn add_options_default() {
        let opts = AddOptions::default();
        assert!(!opts.dev);
        assert!(!opts.optional);
        assert!(!opts.peer);
        assert!(!opts.exact);
    }

    #[test]
    fn install_summary_default() {
        let s = InstallSummary::default();
        assert!(s.added.is_empty());
        assert_eq!(s.bytes_from_cache, 0);
        assert_eq!(s.duration_ms, 0);
    }
}
