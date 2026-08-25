use crate::ecosystem::Ecosystem;
use crate::error::{MgError, MgResult};
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreparedAdd {
    pub id: PackageId,
    pub range: VersionRange,
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
    /// Fail fast if mgc.lock is missing or out-of-sync (CI mode).
    pub frozen: bool,
    /// Skip checking already-installed root packages. Only materialize what differs.
    /// Safe after add/remove when graph changed by exactly one leaf package.
    pub incremental: bool,
    /// Packages to force-install even when incremental mode is active.
    /// Only used when `incremental` is true.
    pub force_install: Vec<PackageId>,
    /// Prefer reusing installed versions (dedupe) instead of latest (02 §2.1).
    /// Opt-in — default off for safety.
    pub prefer_dedupe: bool,
    /// Re-link dangling symlinks in node_modules from the virtual store (02 §2.2).
    pub repair: bool,
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

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" => Self::Critical,
            "high" => Self::High,
            "medium" | "moderate" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Info,
        }
    }
}

impl std::str::FromStr for VulnerabilitySeverity {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str(s))
    }
}

impl From<&str> for VulnerabilitySeverity {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl VulnerabilitySeverity {
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
    /// Regular (non-peer) dependencies.
    pub deps: Vec<PackageId>,
    /// Peer dependencies — resolved to actual graph members.
    /// Populated during the resolution phase so that the layout
    /// materializer does not need a secondary disk read of package.json.
    #[serde(default)]
    pub peer_deps: Vec<PackageId>,
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
    async fn prepare_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PreparedAdd> {
        let exact = opts.exact;
        let mut dry_opts = opts;
        dry_opts.no_save = true;
        let id = self.add(project_root, name, range, dry_opts).await?;
        let saved_range = match range {
            Some(range) if exact => {
                let raw = range
                    .as_str()
                    .trim_start_matches('^')
                    .trim_start_matches('~');
                VersionRange::parse(raw)?
            }
            Some(range) => range.clone(),
            None => VersionRange::star(),
        };
        Ok(PreparedAdd {
            id,
            range: saved_range,
        })
    }
    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()>;
    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>>;
    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>>;
    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport>;

    /// T5 audit --fix: re-resolve the given vulnerable packages to a newer
    /// version and rewrite the lockfile ONLY when re-resolution succeeds
    /// (fail-closed — a failed resolve leaves manifest + lockfile untouched).
    /// Returns the number of packages bumped. Default: unsupported.
    async fn audit_fix(&self, _project_root: &Path, _vulnerable: &[PackageId]) -> MgResult<usize> {
        Err(MgError::Other(
            "audit --fix is not supported for this core".to_string(),
        ))
    }

    /// Enable dedupe preference (reuse installed versions) for the next resolve.
    /// Default no-op — opt-in per install (02 §2.1).
    fn set_dedupe_pref(&self, _enabled: bool) {}

    /// Provide already-installed versions (from lockfile) for dedupe resolution.
    /// Default no-op.
    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {}
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
                peer_deps: vec![],
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
