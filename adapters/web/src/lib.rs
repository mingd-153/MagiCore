#![cfg_attr(test, allow(clippy::unwrap_used))]

//! `adapters/web/src/lib.rs` — Web ecosystem adapter for MagiCore.
//!
//! Provides the primary WebAdapter orchestrating resolution, installation,
//! manifest editing, security audits, and lifecycle hooks for npm/web projects.

use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

use async_trait::async_trait;
use mgc_adapter_base::BaseAdapter;
use mgc_resolver::Resolver as CoreResolver;
use mgc_store::ContentStore;
use mgc_types::{
    adapter::{
        AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
        ResolvedGraph, ResolvedPackage, UpdatedPackage,
    },
    DependencySpec, Manifest, MgResult, PackageId, PackageName, Version, VersionRange,
};

pub mod audit;
pub mod cache;
pub mod cache_daemon;
pub mod install;
pub mod layout;
pub mod lifecycle;
pub mod list;
pub mod lockfile;
pub mod manifest;
pub mod native;
pub mod prefetch;
pub mod profile;
pub mod provider;
pub mod registry_config;
pub mod resolution_cache;
pub mod sbom;
pub mod supply_chain;
pub mod update;

#[cfg(test)]
#[path = "test/unit_tests.rs"]
mod tests;

pub use lockfile::{read_web_lockfile, read_web_lockfile_checked};
pub use manifest::PackageJson;
pub use prefetch::spawn_tarball_download;
pub use registry_config::{
    effective_registry_url, validate_registry_allowed, DEFAULT_NPM_REGISTRY,
};
pub use resolution_cache::manifest_resolution_cache_key;
pub use sbom::generate_sbom;

use crate::audit::{run_audit, run_audit_fix};
use crate::cache::{resolve_prefetch_enabled, SharedWebCache};
use crate::install::run_install;
use crate::lockfile::{build_graph_from_lockfile, lockfile_satisfies_manifest};
use crate::manifest::{parse_manifest, write_manifest};
use crate::profile::ResolveProfile;
use crate::provider::NpmDependencyProvider;
use crate::supply_chain::enforce_resolution_supply_chain_guards;
use crate::update::preferred_registry_version;

pub struct WebAdapter {
    pub registry_url: String,
    pub provider: Arc<NpmDependencyProvider>,
    pub resolver: Arc<CoreResolver>,
    pub store: Option<ContentStore>,
    pub shared_cache: Option<SharedWebCache>,
    pub prefetch_handle: Mutex<Option<tokio::task::JoinHandle<MgResult<u64>>>>,
    dedupe_pref: AtomicBool,
    existing_versions: Mutex<std::collections::HashMap<String, String>>,
}

impl WebAdapter {
    pub fn new() -> Self {
        let registry_url = effective_registry_url(DEFAULT_NPM_REGISTRY);
        let shared_cache = SharedWebCache::discover();
        Self::build(registry_url, None, Vec::new(), shared_cache)
    }

    pub fn with_registry(registry_url: String) -> Self {
        let registry_url = effective_registry_url(&registry_url);
        let shared_cache = SharedWebCache::discover();
        Self::build(registry_url, None, Vec::new(), shared_cache)
    }

    pub fn with_registry_and_token(registry_url: String, token: Option<String>) -> Self {
        let registry_url = effective_registry_url(&registry_url);
        let shared_cache = SharedWebCache::discover();
        Self::build(registry_url, token, Vec::new(), shared_cache)
    }

    pub fn with_registry_chain(
        primary: String,
        token: Option<String>,
        fallbacks: Vec<(String, Option<String>)>,
    ) -> Self {
        let primary = effective_registry_url(&primary);
        let shared_cache = SharedWebCache::discover();
        Self::build(primary, token, fallbacks, shared_cache)
    }

    fn build(
        registry_url: String,
        token: Option<String>,
        fallbacks: Vec<(String, Option<String>)>,
        shared_cache: Option<SharedWebCache>,
    ) -> Self {
        let provider = Arc::new(NpmDependencyProvider::new_with_chain(
            &registry_url,
            token,
            fallbacks,
            shared_cache.clone(),
        ));
        Self {
            registry_url,
            provider: provider.clone(),
            resolver: Arc::new(CoreResolver::new(provider)),
            store: None,
            shared_cache,
            prefetch_handle: Mutex::new(None),
            dedupe_pref: AtomicBool::new(false),
            existing_versions: Mutex::new(std::collections::HashMap::new()),
        }
    }

    #[cfg(test)]
    pub fn with_registry_and_shared_cache(registry_url: String, shared_root: PathBuf) -> Self {
        Self::build(
            registry_url,
            None,
            Vec::new(),
            Some(SharedWebCache { root: shared_root }),
        )
    }

    pub fn with_store(mut self, store: ContentStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn metadata_versions(metadata: &native::npm_registry::PackageMetadata) -> Vec<Version> {
        let mut versions: Vec<Version> = metadata
            .versions
            .keys()
            .filter_map(|k| Version::parse(k).ok())
            .collect();
        versions.sort();
        versions
    }

    async fn infer_add_range(
        &self,
        name: &PackageName,
        explicit_range: Option<&VersionRange>,
        exact: bool,
    ) -> MgResult<VersionRange> {
        let should_fetch = match explicit_range {
            Some(range) => {
                let raw = range.as_str();
                raw == "latest" || raw == "*" || raw.is_empty()
            }
            None => true,
        };

        if should_fetch {
            let registry = native::npm_registry::NpmRegistry::new(&self.registry_url);
            let latest = self.latest_version_string(name, &registry).await?;
            let saved = if exact { latest } else { format!("^{latest}") };
            return VersionRange::parse(&saved);
        }

        if let Some(range) = explicit_range {
            let raw = if exact {
                range
                    .as_str()
                    .trim_start_matches('^')
                    .trim_start_matches('~')
            } else {
                range.as_str()
            };
            return VersionRange::parse(raw);
        }

        unreachable!()
    }

    async fn latest_version_string(
        &self,
        name: &PackageName,
        _registry: &native::npm_registry::NpmRegistry,
    ) -> MgResult<String> {
        let metadata = self
            .provider
            .metadata(name)
            .await
            .map_err(|err| mgc_types::MgError::Network(err.to_string()))?;

        preferred_registry_version(&metadata).ok_or_else(|| {
            mgc_types::MgError::Other(format!(
                "unable to infer latest version for '{}'",
                name.as_str()
            ))
        })
    }

    pub fn preferred_saved_range(current: &VersionRange, latest: &str) -> MgResult<VersionRange> {
        crate::update::preferred_saved_range(current, latest)
    }

    fn existing_versions_guard(&self) -> MutexGuard<'_, std::collections::HashMap<String, String>> {
        self.existing_versions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn prefetch_handle_guard(
        &self,
    ) -> MutexGuard<'_, Option<tokio::task::JoinHandle<MgResult<u64>>>> {
        self.prefetch_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for WebAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseAdapter for WebAdapter {}

#[async_trait]
impl PackageAdapter for WebAdapter {
    fn name(&self) -> &str {
        "web"
    }

    fn ecosystem(&self) -> mgc_types::ecosystem::Ecosystem {
        mgc_types::ecosystem::Ecosystem::Web
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        project_root.join("package.json").exists()
    }

    fn set_dedupe_pref(&self, enabled: bool) {
        self.dedupe_pref
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        self.resolver.set_dedupe_pref(if enabled {
            mgc_resolver::solver::DedupePref::PreferExisting
        } else {
            mgc_resolver::solver::DedupePref::PreferLatest
        });
    }

    fn set_existing_versions(&self, versions: std::collections::HashMap<String, String>) {
        let parsed: std::collections::HashMap<String, Version> = versions
            .iter()
            .filter_map(|(name, v)| Version::parse(v).ok().map(|ver| (name.clone(), ver)))
            .collect();
        *self.existing_versions_guard() = versions;
        self.resolver.set_existing_versions(parsed);
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        parse_manifest(project_root)
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        write_manifest(project_root, manifest)
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        let started_at = std::time::Instant::now();
        let mut profile = ResolveProfile::from_env();
        let wanted: Vec<(PackageName, String)> = manifest
            .all_dependencies()
            .map(|d| {
                if let Some((target, range)) =
                    NpmDependencyProvider::parse_alias_spec(d.range.as_str())
                {
                    if let Ok(target_name) = PackageName::new(target) {
                        self.provider.record_alias_target(&d.name, &target_name);
                        (d.name.clone(), range)
                    } else {
                        (d.name.clone(), d.range.to_string())
                    }
                } else {
                    (d.name.clone(), d.range.to_string())
                }
            })
            .collect();
        profile.mark("collect_wanted", started_at);
        if wanted.is_empty() {
            return Ok(ResolvedGraph::empty());
        }

        if let Some(lockfile) = read_web_lockfile_checked(Path::new("."))? {
            if lockfile_satisfies_manifest(&lockfile, manifest) {
                if let Ok(Some(graph)) = build_graph_from_lockfile(&lockfile, manifest) {
                    profile.mark("lockfile_short_circuit", started_at);
                    profile.flush(started_at.elapsed().as_millis() as u64);
                    return Ok(graph);
                }
            }
        }
        profile.mark("lockfile_check", started_at);

        let resolution_cache_key = self
            .shared_cache
            .as_ref()
            .map(|_| manifest_resolution_cache_key(manifest, &self.registry_url));
        if let (Some(shared_cache), Some(key)) =
            (self.shared_cache.as_ref(), resolution_cache_key.as_deref())
        {
            if let Some(graph) = shared_cache.read_resolution(key, &self.registry_url)? {
                profile.mark("shared_resolution_cache_hit", started_at);
                profile.flush(started_at.elapsed().as_millis() as u64);
                return Ok(graph);
            }
        }
        profile.mark("shared_resolution_cache_check", started_at);

        let solve_started_at = std::time::Instant::now();
        let result = self
            .resolver
            .solve(&wanted)
            .await
            .map_err(|e| mgc_types::MgError::DependencyConflict(e.message))?;
        profile.mark("solver_solve", solve_started_at);

        let metadata_started_at = std::time::Instant::now();
        let metadata = self
            .provider
            .prefetch_resolution_metadata(
                &result
                    .resolutions
                    .iter()
                    .map(|r| r.package_id.name().clone())
                    .collect::<Vec<_>>(),
            )
            .await
            .map_err(|err| mgc_types::MgError::Network(err.to_string()))?;
        profile.mark("prefetch_resolution_metadata", metadata_started_at);
        let index_started_at = std::time::Instant::now();
        let resolution_index: std::collections::HashMap<
            String,
            Vec<&mgc_resolver::solver::Resolution>,
        > = result.resolutions.iter().fold(
            std::collections::HashMap::new(),
            |mut acc, resolution| {
                acc.entry(resolution.package_id.name_str().to_string())
                    .or_default()
                    .push(resolution);
                acc
            },
        );
        profile.mark("build_resolution_index", index_started_at);
        let package_started_at = std::time::Instant::now();
        let packages: Vec<ResolvedPackage> = result
            .resolutions
            .iter()
            .map(|r| {
                let is_direct = manifest.find_dep(r.package_id.name().as_str()).is_some();
                let is_dev = manifest
                    .dev_dependencies
                    .iter()
                    .any(|d| d.name == *r.package_id.name());
                let (tarball_url, integrity) = metadata
                    .get(r.package_id.name_str())
                    .and_then(|package| package.versions.get(&r.package_id.version().to_string()))
                    .and_then(|version| version.dist.as_ref())
                    .map(|dist| {
                        (
                            dist.tarball.clone(),
                            dist.integrity.clone().unwrap_or_default(),
                        )
                    })
                    .unwrap_or_else(|| (String::new(), r.integrity.clone()));
                ResolvedPackage {
                    id: r.package_id.clone(),
                    integrity,
                    tarball_url,
                    deps: r
                        .dep_specs
                        .iter()
                        .filter_map(|(name, spec)| {
                            let constraint = VersionRange::parse(spec).ok()?;
                            let dep_name = PackageName::new(name.as_str()).ok()?;
                            let source = self.provider.source_package_name(&dep_name);
                            resolution_index
                                .get(source.as_str())
                                .and_then(|candidates| {
                                    candidates
                                        .iter()
                                        .filter(|candidate| constraint.matches(&candidate.version))
                                        .max_by(|left, right| left.version.cmp(&right.version))
                                })
                                .map(|candidate| candidate.package_id.clone())
                        })
                        .collect(),
                    peer_deps: metadata
                        .get(r.package_id.name_str())
                        .and_then(|pkg_meta| {
                            pkg_meta.versions.get(&r.package_id.version().to_string())
                        })
                        .and_then(|ver_meta| ver_meta.peer_dependencies.as_ref())
                        .map(|peers| {
                            peers
                                .keys()
                                .filter_map(|peer_name| {
                                    resolution_index.get(peer_name.as_str()).and_then(
                                        |candidates| {
                                            candidates
                                                .iter()
                                                .max_by(|a, b| a.version.cmp(&b.version))
                                        },
                                    )
                                })
                                .map(|candidate| candidate.package_id.clone())
                                .collect()
                        })
                        .unwrap_or_default(),
                    direct: is_direct,
                    dev: is_dev,
                }
            })
            .collect();
        profile.mark("assemble_resolved_packages", package_started_at);

        enforce_resolution_supply_chain_guards(&result.resolutions, &metadata)?;

        if resolve_prefetch_enabled() {
            if let Some(shared_cache) = self.shared_cache.clone() {
                let registry_url = self.registry_url.clone();
                *self.prefetch_handle_guard() = Some(spawn_tarball_download(
                    shared_cache,
                    packages.clone(),
                    registry_url,
                ));
            }
        }
        let graph = ResolvedGraph { packages };
        if let (Some(shared_cache), Some(key)) =
            (self.shared_cache.as_ref(), resolution_cache_key.as_deref())
        {
            let _ = shared_cache.write_resolution(key, &self.registry_url, &graph);
        }
        profile.mark("write_resolution_cache", started_at);
        profile.flush(started_at.elapsed().as_millis() as u64);
        Ok(graph)
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        let reg = native::npm_registry::NpmRegistry::new_with_token(
            &self.registry_url,
            self.provider.registry.auth_token().map(str::to_string),
        );
        for pkg in &graph.packages {
            let unscoped = pkg.id.name().unscoped();
            let url = format!(
                "{}/{}/-/{}-{}.tgz",
                self.registry_url,
                pkg.id.name_str(),
                unscoped,
                pkg.id.version()
            );
            let bytes = reg.download_tarball(&url).await.map_err(|e| {
                mgc_types::MgError::Network(format!(
                    "download failed for '{}': {}",
                    pkg.id.name_str(),
                    e
                ))
            })?;
            if let Some(ref store) = self.store {
                store
                    .import_bytes(&bytes)
                    .map_err(|e| mgc_types::MgError::Store(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        let prefetch_handle = self.prefetch_handle_guard().take();
        run_install(
            &self.registry_url,
            self.provider.registry.auth_token(),
            self.store.as_ref(),
            self.shared_cache.clone(),
            prefetch_handle,
            graph,
            project_root,
            opts,
        )
        .await
    }

    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId> {
        let mut manifest = self.parse_manifest(project_root).await?;
        let inferred = self.infer_add_range(name, range, opts.exact).await?;

        let mut spec = DependencySpec::new(name.clone(), inferred.clone());
        spec.dev = opts.dev;
        spec.optional = opts.optional;
        spec.peer = opts.peer;
        manifest.add_dep(spec, opts.dev, opts.optional, opts.peer);

        if !opts.no_save {
            self.write_manifest(project_root, &manifest).await?;
        }

        let version = inferred
            .satisfying_version()
            .unwrap_or_else(|| Version::new(0, 0, 0));
        Ok(PackageId::new(name.clone(), version))
    }

    async fn prepare_add(
        &self,
        _project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<mgc_types::adapter::PreparedAdd> {
        let inferred = self.infer_add_range(name, range, opts.exact).await?;
        let version = inferred
            .satisfying_version()
            .unwrap_or_else(|| Version::new(0, 0, 0));
        Ok(mgc_types::adapter::PreparedAdd {
            id: PackageId::new(name.clone(), version),
            range: inferred,
        })
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        self.base_remove(project_root, name).await
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        crate::list::run_list(project_root).await
    }

    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        crate::update::run_update(project_root, name, &self.registry_url, &self.provider).await
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        run_audit(project_root, &self.registry_url).await
    }

    async fn audit_fix(&self, project_root: &Path, vulnerable: &[PackageId]) -> MgResult<usize> {
        run_audit_fix(project_root, vulnerable, |m| async move {
            self.resolve(&m).await
        })
        .await
    }
}
