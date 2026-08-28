//! Dependency resolution engine — batch queue, semver matching, error propagation.
//!
//! ## Flow
//! 1. Wanted packages queued from CLI.
//! 2. Each batch prefetches version info.
//! 3. For each package: override → already-resolved-check → fresh resolve.
//! 4. Resolved packages enqueue their own transitive deps.
//!
//! ## Error Policy (v2)
//! `DependencyProvider` now returns `Result`. Errors propagate — no silent skips.
//!
//! ## Limitations
//! - No PubGrub backtracking for conflicts
//! - No multi-version hoisting

pub mod pubgrub;

use async_trait::async_trait;
use futures_util::future::{join, join_all};
use mgc_types::{PackageId, PackageName, Version, VersionRange};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

pub use pubgrub::{Cause, Incompatibility, PubGrubSolver, SolveError as PubGrubSolveError, Term};

/// Error from a `DependencyProvider` (network failure, registry 500, etc.).
#[derive(Debug, Clone)]
pub struct DependencyError(pub String);

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for DependencyError {}

/// One dependency of a resolved package: name + version spec + flags.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub package: PackageName,
    pub spec: String,
    pub optional: bool,
    pub peer: bool,
}

/// A single resolution entry: pinned package + version + transitive deps.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub package_id: PackageId,
    pub version: Version,
    pub integrity: String,
    pub deps: Vec<String>,
    pub dep_specs: Vec<(String, String)>,
    /// Names of peer dependencies (used for peer instance merge — 02 §2.1)
    pub peer_deps: Vec<String>,
}

/// Version selection preference for dedupe (02 §2.1 — Q8: default safe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DedupePref {
    /// Pick the newest matching version (current behaviour, default).
    #[default]
    PreferLatest,
    /// Prefer a version already present in the graph/lockfile when it satisfies the range.
    PreferExisting,
}

/// Complete resolution result: all packages in topo-ish order.
#[derive(Debug, Clone)]
pub struct SolveResult {
    pub resolutions: Vec<Resolution>,
}

/// Resolution failure: conflict or provider error.
#[derive(Debug, Clone)]
pub struct SolveError {
    pub message: String,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for SolveError {}
impl From<String> for SolveError {
    fn from(s: String) -> Self {
        Self { message: s }
    }
}

/// Dependency provider trait — each ecosystem must implement this.
///
/// ⚠ Both methods return `Result`. On network failure, error propagates.
///    Empty vec is valid (package exists with no versions / no deps).
#[async_trait]
pub trait DependencyProvider: Send + Sync {
    async fn get_versions(&self, package: &PackageName) -> Result<Vec<Version>, DependencyError>;
    async fn get_dependencies(&self, id: &PackageId) -> Result<Vec<ResolvedDep>, DependencyError>;
    async fn should_enqueue(&self, dep: &ResolvedDep) -> Result<bool, DependencyError> {
        let _ = dep;
        Ok(true)
    }

    async fn prefetch_versions(
        &self,
        packages: &[PackageName],
    ) -> Result<Vec<(PackageName, Vec<Version>)>, DependencyError> {
        if packages.is_empty() {
            return Ok(Vec::new());
        }
        // Fire all version lookups concurrently — eliminates the N×latency serial bottleneck.
        let futures: Vec<_> = packages
            .iter()
            .map(|name| {
                let name = name.clone();
                async move {
                    let versions = self.get_versions(&name).await?;
                    Ok::<_, DependencyError>((name, versions))
                }
            })
            .collect();
        let results = join_all(futures).await;
        results.into_iter().collect()
    }

    async fn prefetch_dependencies(
        &self,
        ids: &[PackageId],
    ) -> Result<Vec<(PackageId, Vec<ResolvedDep>)>, DependencyError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // Fire all dependency fetches concurrently.
        let futures: Vec<_> = ids
            .iter()
            .map(|id| {
                let id = id.clone();
                async move {
                    let deps = self.get_dependencies(&id).await?;
                    Ok::<_, DependencyError>((id, deps))
                }
            })
            .collect();
        let results = join_all(futures).await;
        results.into_iter().collect()
    }

    /// Called by solver after each batch resolves exact package versions.
    /// Provider can use this to start background work (e.g. tarball download).
    async fn on_batch_resolved(&self, packages: &[PackageId]) -> Result<(), DependencyError> {
        let _ = packages;
        Ok(())
    }
}

/// Info about a dependency, used by `check_dependency_confusion`.
#[derive(Debug, Clone)]
pub struct DepInfo {
    pub name: String,
    pub version: Option<String>,
    pub registry: Option<String>,
}

/// Check for dependency confusion attacks:
/// - Workspace package vs external dep same name.
/// - Scoped package wrong registry.
/// - Package from untrusted registry.
/// - Typosquatting against known popular packages.
pub fn check_dependency_confusion(
    workspace_packages: &[String],
    dependencies: &[DepInfo],
    scoped_registries: &HashMap<String, String>,
    trusted_registries: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();

    // Top 50 most depended-upon npm packages (for typosquat detection)
    const TOP_NPM: &[&str] = &[
        "lodash",
        "chalk",
        "react",
        "express",
        "axios",
        "moment",
        "uuid",
        "tslib",
        "commander",
        "prettier",
        "typescript",
        "eslint",
        "webpack",
        "babel",
        "jest",
        "mocha",
        "sinon",
        "async",
        "request",
        "body-parser",
        "cors",
        "debug",
        "dotenv",
        "glob",
        "http-errors",
        "iconv-lite",
        "isarray",
        "js-yaml",
        "json5",
        "jsonwebtoken",
        "ms",
        "node-fetch",
        "once",
        "path-to-regexp",
        "qs",
        "raw-body",
        "readable-stream",
        "safe-buffer",
        "semver",
        "send",
        "serve-static",
        "setprototypeof",
        "statuses",
        "supports-color",
        "through2",
        "underscore",
        "yargs",
        "vue",
        "next",
        "nuxt",
    ];

    for dep in dependencies {
        if workspace_packages.contains(&dep.name) && dep.version.is_some() {
            warnings.push(format!(
                    "Dependency confusion: '{}' is both workspace package and external dep. Use \"workspace:*\".", dep.name
                ));
        }
        if dep.name.starts_with('@') {
            if let Some(scope) = dep.name.split('/').next() {
                if let Some(expected) = scoped_registries.get(scope) {
                    if dep.registry.as_deref() != Some(expected.as_str()) {
                        warnings.push(format!(
                            "Dependency confusion: '{}' should resolve from '{}' but resolves from '{}'",
                            dep.name, expected, dep.registry.as_deref().unwrap_or("public npm")
                        ));
                    }
                }
            }
        }
        if !trusted_registries.is_empty() {
            if let Some(ref reg) = dep.registry {
                if !trusted_registries.contains(reg) {
                    warnings.push(format!(
                        "Dependency confusion: '{}' from '{}' not in trusted registries",
                        dep.name, reg
                    ));
                }
            }
        }
        // Typosquat check: Levenshtein distance against top npm packages
        if !dep.name.starts_with('@') && !dep.name.contains('/') {
            for &popular in TOP_NPM {
                let dist = levenshtein_distance(&dep.name, popular);
                if dist == 1 && dep.name.len() >= 4 {
                    warnings.push(format!(
                        "Typosquat warning: '{}' (distance 1 from '{}')",
                        dep.name, popular
                    ));
                } else if dist == 2 && dep.name.len() >= 6 {
                    warnings.push(format!(
                        "Typosquat warning: '{}' (distance 2 from '{}')",
                        dep.name, popular
                    ));
                }
            }
        }
    }
    warnings
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] =
                std::cmp::min(std::cmp::min(curr[j] + 1, prev[j + 1] + 1), prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

/// Batch resolver with error propagation.
///
/// ## Algorithm
/// 1. Enqueue initial deps from CLI.
/// 2. Drain batch (max 50), prefetch unique names.
/// 3. Per package:
///    a. Override check.
///    b. Already resolved → recheck, upgrade within major if needed.
///    c. Fresh resolve → pick latest matching version.
/// 4. Enqueue resolved deps.
/// 5. Repeat until queue empty.
///
/// ## Errors
/// Provider errors → immediate `SolveError`. No silent skips.
pub struct Resolver {
    provider: std::sync::Arc<dyn DependencyProvider>,
    overrides: HashMap<String, String>,
    /// Version selection preference (default PreferLatest = safe).
    /// RwLock so shared resolvers (Arc in adapters) can be reconfigured per solve.
    dedupe_pref: std::sync::RwLock<DedupePref>,
    /// Versions already installed (from lockfile) to prefer under PreferExisting.
    existing_versions: std::sync::RwLock<HashMap<String, Version>>,
    /// G2: Peer-deps cache — memoizes resolved dependencies by PackageId.
    /// Key: PackageId string, Value: Arc<[ResolvedDep]>
    peer_cache: std::sync::RwLock<HashMap<String, Arc<[ResolvedDep]>>>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolver")
            .field("overrides", &self.overrides)
            .field("dedupe_pref", &self.dedupe_pref.read().unwrap())
            .field("existing_versions", &self.existing_versions.read().unwrap())
            .finish()
    }
}

#[derive(Default)]
struct ResolverProfile {
    enabled: bool,
}

impl ResolverProfile {
    fn from_env() -> Self {
        let enabled = std::env::var("MAGICORE_RESOLVER_PROFILE")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Self { enabled }
    }

    fn mark(&self, label: &str, started_at: Instant) {
        if self.enabled {
            eprintln!(
                "[magicore:resolver-profile] {}={}ms",
                label,
                started_at.elapsed().as_millis()
            );
        }
    }
}

impl Resolver {
    pub fn new(provider: std::sync::Arc<dyn DependencyProvider>) -> Self {
        Self {
            provider,
            overrides: HashMap::new(),
            dedupe_pref: std::sync::RwLock::new(DedupePref::default()),
            existing_versions: std::sync::RwLock::new(HashMap::new()),
            peer_cache: std::sync::RwLock::new(HashMap::new()),
        }
    }

    pub fn set_overrides(&mut self, overrides: HashMap<String, String>) {
        self.overrides = overrides;
    }

    /// Set dedupe preference (02 §2.1). Default is PreferLatest (safe).
    pub fn set_dedupe_pref(&self, pref: DedupePref) {
        *self.dedupe_pref.write().unwrap() = pref;
    }

    /// Provide versions already present in the project (from lockfile) so
    /// PreferExisting can reuse them instead of installing new instances.
    pub fn set_existing_versions(&self, existing: HashMap<String, Version>) {
        *self.existing_versions.write().unwrap() = existing;
    }

    /// Pick the version to use: under PreferExisting, reuse the installed
    /// version when it satisfies the constraint (dedupe — 02 §2.1).
    fn select_version(
        &self,
        name: &str,
        constraint: &VersionRange,
        spec: &str,
        versions: &[Version],
    ) -> Option<Version> {
        if *self.dedupe_pref.read().unwrap() == DedupePref::PreferExisting {
            if let Some(existing) = self.existing_versions.read().unwrap().get(name) {
                if constraint.matches(existing) {
                    return Some(existing.clone());
                }
            }
        }
        Self::select_best_version(versions, constraint, spec)
    }

    fn select_best_version(
        versions: &[Version],
        constraint: &VersionRange,
        spec: &str,
    ) -> Option<Version> {
        let mut matches: Vec<Version> = versions
            .iter()
            .filter(|v| constraint.matches(v))
            .cloned()
            .collect();
        if matches.is_empty() {
            return None;
        }

        let allows_prerelease = spec.contains('-');
        if !allows_prerelease {
            let stable: Vec<Version> = matches
                .iter()
                .filter(|v| v.pre.is_none())
                .cloned()
                .collect();
            if let Some(best) = stable.into_iter().max() {
                return Some(best);
            }
        }

        matches.sort();
        matches.into_iter().max()
    }

    fn versions_to_map(
        entries: Vec<(PackageName, Vec<Version>)>,
    ) -> HashMap<String, Arc<[Version]>> {
        entries
            .into_iter()
            .map(|(name, versions)| (name.as_str().to_string(), Arc::<[Version]>::from(versions)))
            .collect()
    }

    /// Resolve all dependencies. Fails with `SolveError` on provider error or
    /// unresolvable constraint.
    pub async fn solve(&self, wanted: &[(PackageName, String)]) -> Result<SolveResult, SolveError> {
        const MAX_PACKAGES: usize = 10000;
        const MAX_QUEUE_SIZE: usize = 50000;

        let solve_started_at = Instant::now();
        let profile = ResolverProfile::from_env();
        let mut resolutions: Vec<Resolution> = Vec::new();
        let mut resolved: HashMap<String, Version> = HashMap::new();
        let mut queue: VecDeque<(PackageName, String)> =
            wanted.iter().map(|(n, s)| (n.clone(), s.clone())).collect();
        let mut resolved_versions: HashSet<(String, Version)> = HashSet::new();

        // Pre-fetch initial batch
        let initial_names: Vec<PackageName> = wanted.iter().map(|(n, _)| n.clone()).collect();
        let initial_prefetch_started_at = Instant::now();
        let initial_versions = self
            .provider
            .prefetch_versions(&initial_names)
            .await
            .map_err(|e| SolveError {
                message: format!("initial prefetch failed: {e}"),
            })?;
        profile.mark("initial_prefetch_versions", initial_prefetch_started_at);
        let mut prefetched_versions = Self::versions_to_map(initial_versions);

        while !queue.is_empty() {
            if resolutions.len() >= MAX_PACKAGES {
                return Err(SolveError {
                    message: format!(
                        "dependency graph too large: exceeded {} packages (possible circular dependency or dep bomb)",
                        MAX_PACKAGES
                    ),
                });
            }

            if queue.len() > MAX_QUEUE_SIZE {
                return Err(SolveError {
                    message: format!(
                        "dependency queue overflow: exceeded {} entries (possible dep bomb attack)",
                        MAX_QUEUE_SIZE
                    ),
                });
            }

            let batch_limit = std::env::var("MAGICORE_RESOLVER_BATCH_SIZE")
                .ok()
                .and_then(|raw| raw.trim().parse::<usize>().ok())
                .filter(|limit| *limit > 0)
                .unwrap_or(1024);
            let batch_size = queue.len().min(batch_limit);
            let batch: Vec<(PackageName, String)> = queue.drain(..batch_size).collect();
            let mut selected: Vec<(PackageName, String, Version)> = Vec::new();
            let batch_started_at = Instant::now();

            // Deduplicate and prefetch batch versions
            let mut seen = HashSet::new();
            let batch_names: Vec<PackageName> = batch
                .iter()
                .filter(|(n, _)| {
                    let name = n.as_str().to_string();
                    seen.insert(name.clone()) && !prefetched_versions.contains_key(&name)
                })
                .map(|(n, _)| n.clone())
                .collect();
            if !batch_names.is_empty() {
                let batch_prefetch_started_at = Instant::now();
                prefetched_versions.extend(Self::versions_to_map(
                    self.provider
                        .prefetch_versions(&batch_names)
                        .await
                        .map_err(|e| SolveError {
                            message: format!("batch prefetch failed: {e}"),
                        })?,
                ));
                profile.mark("batch_prefetch_versions", batch_prefetch_started_at);
            }

            let select_started_at = Instant::now();
            for (name, spec) in batch {
                let name_str = name.as_str().to_string();
                let constraint = VersionRange::parse(&spec).map_err(|e| SolveError {
                    message: format!("invalid spec '{}' for '{}': {}", spec, name_str, e),
                })?;

                // Phase 1: override
                if let Some(override_spec) = self.overrides.get(&name_str) {
                    let oc = VersionRange::parse(override_spec).map_err(|e| SolveError {
                        message: format!("invalid override '{}': {}", override_spec, e),
                    })?;
                    let versions = if let Some(prefetched) = prefetched_versions.get(&name_str) {
                        Arc::clone(prefetched)
                    } else {
                        Arc::<[Version]>::from(self.provider.get_versions(&name).await.map_err(
                            |e| SolveError {
                                message: format!("versions fetch failed for '{}': {}", name_str, e),
                            },
                        )?)
                    };
                    if let Some(v) =
                        Self::select_best_version(versions.as_ref(), &oc, override_spec)
                            .or_else(|| versions.iter().filter(|v| v.pre.is_none()).cloned().max())
                            .or_else(|| versions.iter().cloned().max())
                    {
                        resolved.insert(name_str.clone(), v.clone());
                        resolved_versions.insert((name_str.clone(), v.clone()));
                        selected.push((name, name_str, v));
                    }
                    continue;
                }

                // Phase 2: already resolved
                if let Some(existing) = resolved.get(&name_str).cloned() {
                    if constraint.matches(&existing) {
                        continue;
                    }
                    let versions = if let Some(prefetched) = prefetched_versions.get(&name_str) {
                        Arc::clone(prefetched)
                    } else {
                        Arc::<[Version]>::from(self.provider.get_versions(&name).await.map_err(
                            |e| SolveError {
                                message: format!("cannot fetch versions for '{}': {}", name_str, e),
                            },
                        )?)
                    };

                    if let Some(other) =
                        Self::select_best_version(versions.as_ref(), &constraint, &spec)
                    {
                        let key = (name_str.clone(), other.clone());
                        if !resolved_versions.contains(&key) {
                            resolved_versions.insert(key);
                            selected.push((name, name_str, other));
                        }
                    }
                    continue;
                }

                // Phase 3: fresh resolve
                let versions = if let Some(prefetched) = prefetched_versions.get(&name_str) {
                    Arc::clone(prefetched)
                } else {
                    Arc::<[Version]>::from(self.provider.get_versions(&name).await.map_err(
                        |e| SolveError {
                            message: format!("cannot fetch versions for '{}': {}", name_str, e),
                        },
                    )?)
                };
                let version = self.select_version(&name_str, &constraint, &spec, versions.as_ref());

                match version {
                    Some(v) => {
                        resolved.insert(name_str.clone(), v.clone());
                        resolved_versions.insert((name_str.clone(), v.clone()));
                        selected.push((name, name_str, v));
                    }
                    None => {
                        return Err(SolveError {
                            message: format!("no version of '{}' matches '{}'", name_str, spec),
                        })
                    }
                }
            }
            profile.mark("select_versions", select_started_at);

            if !selected.is_empty() {
                let deps_prefetch_started_at = Instant::now();
                let ids: Vec<PackageId> = selected
                    .iter()
                    .map(|(name, _, version)| PackageId::new(name.clone(), version.clone()))
                    .collect();
                
                // G2: Peer cache — check cache before prefetch
                let mut dependency_results = HashMap::new();
                let mut uncached_ids = Vec::new();
                {
                    let cache = self.peer_cache.read().unwrap();
                    for id in &ids {
                        let key = format!("{}@{}", id.name_str(), id.version());
                        if let Some(cached_deps) = cache.get(&key) {
                            dependency_results.insert(id.clone(), cached_deps.to_vec());
                        } else {
                            uncached_ids.push(id.clone());
                        }
                    }
                }
                
                // Fetch uncached dependencies
                if !uncached_ids.is_empty() {
                    let fetched = self.provider
                        .prefetch_dependencies(&uncached_ids)
                        .await
                        .map_err(|e| SolveError {
                            message: format!("dependency prefetch failed: {e}"),
                        })?;
                    
                    // Store in cache
                    let mut cache = self.peer_cache.write().unwrap();
                    for (id, deps) in fetched {
                        let key = format!("{}@{}", id.name_str(), id.version());
                        let deps_arc = Arc::<[ResolvedDep]>::from(deps.clone());
                        cache.insert(key, deps_arc.clone());
                        dependency_results.insert(id, deps);
                    }
                }
                
                profile.mark("prefetch_dependencies", deps_prefetch_started_at);
                self.provider
                    .on_batch_resolved(&ids)
                    .await
                    .map_err(|e| SolveError {
                        message: format!("on_batch_resolved hook failed: {e}"),
                    })?;
                let prefetched_dependencies: HashMap<PackageId, Arc<[ResolvedDep]>> =
                    dependency_results
                        .into_iter()
                        .map(|(id, deps)| (id, Arc::<[ResolvedDep]>::from(deps)))
                        .collect();
                let mut next_prefetch_seen = HashSet::new();
                let next_prefetch_names: Vec<PackageName> = prefetched_dependencies
                    .values()
                    .flat_map(|deps| deps.iter())
                    .filter_map(|dep| {
                        let name = dep.package.as_str().to_string();
                        if prefetched_versions.contains_key(&name)
                            || !next_prefetch_seen.insert(name)
                        {
                            None
                        } else {
                            Some(dep.package.clone())
                        }
                    })
                    .collect();
                let add_resolution_started_at = Instant::now();
                let add_resolution_future = async {
                    let mut local_resolutions = Vec::new();
                    let mut local_queue = VecDeque::new();
                    for (name, name_str, version) in selected {
                        let pid = PackageId::new(name.clone(), version.clone());
                        let deps = prefetched_dependencies
                            .get(&pid)
                            .ok_or_else(|| SolveError {
                                message: format!(
                                    "dependency prefetch missing result for '{}'",
                                    pid
                                ),
                            })?;
                        Self::add_resolution(
                            &mut local_resolutions,
                            &name,
                            &name_str,
                            version,
                            deps.as_ref(),
                            self.provider.as_ref(),
                            &mut local_queue,
                        )
                        .await?;
                    }
                    Ok::<_, SolveError>((local_resolutions, local_queue))
                };
                let (add_resolution_result, prefetched_next_result) =
                    if next_prefetch_names.is_empty() {
                        (add_resolution_future.await, Ok(Vec::new()))
                    } else {
                        join(
                            add_resolution_future,
                            self.provider.prefetch_versions(&next_prefetch_names),
                        )
                        .await
                    };
                let (new_resolutions, new_queue) = add_resolution_result?;
                resolutions.extend(new_resolutions);
                queue.extend(new_queue);
                profile.mark("add_resolution", add_resolution_started_at);
                prefetched_versions.extend(Self::versions_to_map(prefetched_next_result.map_err(
                    |e| SolveError {
                        message: format!("next-wave prefetch failed: {e}"),
                    },
                )?));
            }
            profile.mark("batch_total", batch_started_at);
        }

        profile.mark("solve_total", solve_started_at);
        Ok(SolveResult {
            resolutions: Self::merge_peer_instances(resolutions),
        })
    }

    /// Record a resolved package and enqueue its transitive dependencies.
    async fn add_resolution(
        resolutions: &mut Vec<Resolution>,
        name: &PackageName,
        _name_str: &str,
        version: Version,
        deps: &[ResolvedDep],
        provider: &dyn DependencyProvider,
        queue: &mut VecDeque<(PackageName, String)>,
    ) -> Result<(), SolveError> {
        let pid = PackageId::new(name.clone(), version.clone());
        let dep_names: Vec<String> = deps
            .iter()
            .map(|d| d.package.as_str().to_string())
            .collect();
        let dep_specs: Vec<(String, String)> = deps
            .iter()
            .map(|d| (d.package.as_str().to_string(), d.spec.clone()))
            .collect();
        let peer_deps: Vec<String> = deps
            .iter()
            .filter(|d| d.peer)
            .map(|d| d.package.as_str().to_string())
            .collect();
        let enqueue_results = join_all(deps.iter().map(|dep| provider.should_enqueue(dep))).await;
        for (dep, should_enqueue) in deps.iter().zip(enqueue_results) {
            if should_enqueue.map_err(|e| SolveError {
                message: format!(
                    "dependency enqueue check failed for '{}@{}': {}",
                    dep.package.as_str(),
                    dep.spec,
                    e
                ),
            })? {
                queue.push_front((dep.package.clone(), dep.spec.clone()));
            }
        }

        resolutions.push(Resolution {
            package_id: pid,
            version,
            integrity: String::new(),
            deps: dep_names,
            dep_specs,
            peer_deps,
        });

        Ok(())
    }

    /// Peer instance merge post-solve (02 §2.1): merge resolutions that share
    /// the same (name, version) — same peer signature — into one instance.
    /// Safe: the solver already dedupes (name, version) pairs; this pass merges
    /// dep lists when the same instance was recorded more than once.
    fn merge_peer_instances(resolutions: Vec<Resolution>) -> Vec<Resolution> {
        let mut merged: HashMap<(String, String), Resolution> = HashMap::new();
        for r in resolutions {
            let key = (
                r.package_id.name_str().to_string(),
                r.package_id.version().to_string(),
            );
            match merged.get_mut(&key) {
                Some(existing) => {
                    for dep in &r.deps {
                        if !existing.deps.contains(dep) {
                            existing.deps.push(dep.clone());
                        }
                    }
                    for (name, spec) in &r.dep_specs {
                        if !existing.dep_specs.contains(&(name.clone(), spec.clone())) {
                            existing.dep_specs.push((name.clone(), spec.clone()));
                        }
                    }
                }
                None => {
                    merged.insert(key, r);
                }
            }
        }
        merged.into_values().collect()
    }
}

