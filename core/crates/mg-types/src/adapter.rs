//! PackageAdapter trait — the core interface for ALL ecosystem adapters.
//!
//! Every adapter (web, game, ai, cloud, iot) implements this trait.
//! The unified CLI calls these methods without knowing which ecosystem
//! it is talking to.

use std::path::{Path, PathBuf};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::error::MgResult;
use crate::manifest::Manifest;
use crate::package::{PackageId, PackageName, VersionRange};
use crate::version::Version;
use crate::ecosystem::Ecosystem;

// ─── Core trait ──────────────────────────────────────────────────────────────

/// Every ecosystem adapter must implement this trait.
///
/// # Design Principles
/// - Methods are `async` for non-blocking I/O
/// - All paths are absolute or relative to `project_root`
/// - Errors are typed via `MgError`
/// - `can_handle` enables auto-detection of project type
#[async_trait]
pub trait PackageAdapter: Send + Sync {
    // ── Identity ─────────────────────────────────────────────────────────────

    /// Short name of this adapter (e.g. "web", "game", "ai")
    fn name(&self) -> &str;

    /// Ecosystem this adapter belongs to
    fn ecosystem(&self) -> Ecosystem;

    /// Returns true if this adapter can handle the given project directory.
    /// Used for auto-detection when user runs `mg install` without specifying adapter.
    fn can_handle(&self, project_root: &Path) -> bool;

    // ── Manifest ─────────────────────────────────────────────────────────────

    /// Parse the project manifest (package.json / Cargo.toml / pyproject.toml / etc.)
    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest>;

    /// Write an updated manifest back to disk
    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()>;

    // ── Resolution ───────────────────────────────────────────────────────────

    /// Resolve all dependencies and return the full dependency graph
    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph>;

    // ── Fetch ────────────────────────────────────────────────────────────────

    /// Download and cache all packages in the resolved graph
    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()>;

    // ── Install ───────────────────────────────────────────────────────────────

    /// Install packages into the project (link from store → project)
    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
    ) -> MgResult<InstallSummary>;

    // ── Mutation ─────────────────────────────────────────────────────────────

    /// Add a new dependency to the project
    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        dev: bool,
    ) -> MgResult<PackageId>;

    /// Update a package (or all packages if name is None)
    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>>;

    /// Remove a package from the project
    async fn remove(
        &self,
        project_root: &Path,
        name: &PackageName,
    ) -> MgResult<()>;

    // ── Listing ──────────────────────────────────────────────────────────────

    /// List currently installed packages
    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>>;

    // ── Security ─────────────────────────────────────────────────────────────

    /// Run a security audit and return any issues
    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport>;
}

// ─── Supporting types ─────────────────────────────────────────────────────────

/// A fully resolved dependency graph ready for installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedGraph {
    /// All packages in topological order (deps before dependents)
    pub packages: Vec<ResolvedPackage>,
}

impl ResolvedGraph {
    pub fn empty() -> Self { Self { packages: Vec::new() } }
    pub fn len(&self) -> usize { self.packages.len() }
    pub fn is_empty(&self) -> bool { self.packages.is_empty() }
}

/// A single resolved package within a dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPackage {
    pub id: PackageId,
    /// Integrity hash (SRI format: sha256-<base64>)
    pub integrity: String,
    /// Download URL
    pub tarball_url: String,
    /// Direct dependencies of this package
    pub deps: Vec<PackageId>,
    /// True if this is a direct project dependency
    pub direct: bool,
    /// True if dev-only
    pub dev: bool,
}

/// Summary returned after `install()`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallSummary {
    pub added:   Vec<PackageId>,
    pub updated: Vec<PackageId>,
    pub removed: Vec<PackageId>,
    pub unchanged: usize,
    pub duration_ms: u64,
    pub bytes_downloaded: u64,
    pub bytes_from_cache: u64,
}

impl InstallSummary {
    pub fn total(&self) -> usize {
        self.added.len() + self.updated.len() + self.unchanged
    }
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.bytes_downloaded + self.bytes_from_cache;
        if total == 0 { return 0.0; }
        self.bytes_from_cache as f64 / total as f64 * 100.0
    }
}

/// Information about a currently installed package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub id: PackageId,
    pub path: PathBuf,
    pub integrity: Option<String>,
    pub is_direct: bool,
    pub is_dev: bool,
}

/// A package that was updated during `update()`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatedPackage {
    pub name: PackageName,
    pub from_version: Version,
    pub to_version: Version,
}

/// Result of a security audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub packages_audited: usize,
    pub vulnerabilities: Vec<Vulnerability>,
}

impl AuditReport {
    pub fn clean(packages_audited: usize) -> Self {
        Self { packages_audited, vulnerabilities: Vec::new() }
    }
    pub fn is_clean(&self) -> bool { self.vulnerabilities.is_empty() }
    pub fn critical_count(&self) -> usize {
        self.vulnerabilities.iter().filter(|v| v.severity == Severity::Critical).count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    pub package: PackageId,
    pub cve: String,
    pub severity: Severity,
    pub title: String,
    pub fix_available: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Moderate,
    High,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Low      => write!(f, "low"),
            Severity::Moderate => write!(f, "moderate"),
            Severity::High     => write!(f, "high"),
            Severity::Critical => write!(f, "critical"),
        }
    }
}
