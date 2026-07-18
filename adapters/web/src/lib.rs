use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use async_trait::async_trait;
use base64::Engine;
use dashmap::DashMap;
use futures_util::future::join_all;
use mg_adapter_base::BaseAdapter;
use mg_fetcher::extract::extract_tarball_from_reader;
use mg_lockfile::{serialization, LockPackage, Lockfile, ResolutionMeta};
use mg_resolver::{
    DependencyError, DependencyProvider, RegistryCache, ResolvedDep, Resolver as CoreResolver,
};
use mg_store::{ContentStore, Database, Layout, PackageCache};
use mg_types::{
    adapter::{
        AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
        ResolvedGraph, ResolvedPackage, UpdatedPackage,
    },
    DependencySpec, Manifest, MgResult, PackageId, PackageName, Version, VersionRange,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256, Sha512};
use walkdir::WalkDir;

pub mod layout;
pub mod lifecycle;
pub mod native;

const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";

#[derive(Default)]
struct InstallProfile {
    enabled: bool,
    marks: Vec<(&'static str, u128)>,
}

impl InstallProfile {
    fn from_env() -> Self {
        let enabled = std::env::var("MEGAGATE_WEB_PROFILE_INSTALL")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Self {
            enabled,
            marks: Vec::new(),
        }
    }

    fn mark(&mut self, label: &'static str, started_at: std::time::Instant) {
        if self.enabled {
            self.marks.push((label, started_at.elapsed().as_millis()));
        }
    }

    fn flush(&self, total_ms: u64) {
        if !self.enabled {
            return;
        }

        eprintln!("[megagate:web:install-profile] total={}ms", total_ms);
        for (label, millis) in &self.marks {
            eprintln!("[megagate:web:install-profile] {}={}ms", label, millis);
        }
    }
}

fn atomic_write(path: &Path, data: &[u8]) -> MgResult<()> {
    let dir = path.parent().unwrap_or(Path::new("."));

    let tmp_path = dir.join(format!(
        ".mg-tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    if path.exists() {
        let backup_path = path.with_extension("bak");
        let _ = std::fs::copy(path, &backup_path);
    }

    std::fs::write(&tmp_path, data).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        mg_types::MgError::Other(format!("failed to write temp file: {e}"))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)
            .map(|m| m.permissions())
            .unwrap_or_else(|_| std::fs::Permissions::from_mode(0o644));
        perms.set_mode(0o644);
        let _ = std::fs::set_permissions(&tmp_path, perms);
    }

    std::fs::rename(&tmp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        mg_types::MgError::Other(format!("failed to rename temp file: {e}"))
    })?;

    Ok(())
}

fn project_cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(".megagate").join("cache").join("web")
}

pub struct WebAdapter {
    registry_url: String,
    provider: Arc<NpmDependencyProvider>,
    resolver: Arc<CoreResolver>,
    store: Option<ContentStore>,
    shared_cache: Option<SharedWebCache>,
}

impl WebAdapter {
    pub fn new() -> Self {
        let registry_url = effective_registry_url(DEFAULT_NPM_REGISTRY);
        let shared_cache = SharedWebCache::discover();
        Self::build(registry_url, shared_cache)
    }
    pub fn with_registry(registry_url: String) -> Self {
        let registry_url = effective_registry_url(&registry_url);
        let shared_cache = SharedWebCache::discover();
        Self::build(registry_url, shared_cache)
    }

    fn build(registry_url: String, shared_cache: Option<SharedWebCache>) -> Self {
        let provider = Arc::new(NpmDependencyProvider::new(
            &registry_url,
            shared_cache.clone(),
        ));
        Self {
            registry_url,
            provider: provider.clone(),
            resolver: Arc::new(CoreResolver::new(provider)),
            store: None,
            shared_cache,
        }
    }

    #[cfg(test)]
    fn with_registry_and_shared_cache(registry_url: String, shared_root: PathBuf) -> Self {
        Self::build(registry_url, Some(SharedWebCache { root: shared_root }))
    }
    pub fn with_store(mut self, store: ContentStore) -> Self {
        self.store = Some(store);
        self
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
        registry: &native::npm_registry::NpmRegistry,
    ) -> MgResult<String> {
        let metadata = load_metadata_with_fallback(name, registry, self.shared_cache.as_ref())
            .await
            .map_err(|err| mg_types::MgError::Network(err.to_string()))?;

        preferred_registry_version(&metadata).ok_or_else(|| {
            mg_types::MgError::Other(format!(
                "unable to infer latest version for '{}'",
                name.as_str()
            ))
        })
    }

    fn preferred_saved_range(current: &VersionRange, latest: &str) -> MgResult<VersionRange> {
        let raw = current.as_str();
        let next = if raw.starts_with('^') {
            format!("^{latest}")
        } else if raw.starts_with('~') {
            format!("~{latest}")
        } else if raw == "*" {
            format!("^{latest}")
        } else {
            latest.to_string()
        };
        VersionRange::parse(&next)
    }
}

fn effective_registry_url(default: &str) -> String {
    let url = std::env::var("MEGAGATE_WEB_REGISTRY_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string());
    if !url.starts_with("https://") && !allow_insecure_loopback_url(&url) {
        panic!(
            "registry URL must use HTTPS: '{url}' (set MEGAGATE_WEB_REGISTRY_URL to an HTTPS URL)"
        );
    }
    validate_registry_allowed(&url);
    url
}

fn allow_insecure_loopback_url(url: &str) -> bool {
    #[cfg(test)]
    {
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        if parsed.scheme() != "http" {
            return false;
        }
        return matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1")
        );
    }

    #[cfg(not(test))]
    {
        let flag = std::env::var("MEGAGATE_WEB_ALLOW_INSECURE_LOCALHOST")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_default();
        let enabled = matches!(flag.as_str(), "1" | "true" | "yes" | "on");
        if !enabled {
            return false;
        }

        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        if parsed.scheme() != "http" {
            return false;
        }

        matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1")
        )
    }
}

fn validate_registry_allowed(url: &str) {
    let Some(allowed) = std::env::var("MEGAGATE_WEB_ALLOWED_REGISTRIES").ok() else {
        return;
    };
    let allowed_list: Vec<&str> = allowed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if allowed_list.is_empty() {
        return;
    }
    let normalized = url.trim_end_matches('/');
    let matched = allowed_list
        .iter()
        .any(|a| normalized == a.trim_end_matches('/'));
    if matched {
        return;
    }
    panic!(
        "registry '{}' is not in MEGAGATE_WEB_ALLOWED_REGISTRIES ({})",
        url, allowed
    );
}

fn manifest_resolution_cache_key(manifest: &Manifest, registry_url: &str) -> String {
    let mut entries = Vec::new();
    for (group, deps) in manifest.dep_groups() {
        for dep in deps {
            entries.push(format!(
                "{}\0{}\0{}\0{}\0{}\0{}",
                group,
                dep.name.as_str(),
                dep.range.as_str(),
                dep.dev,
                dep.optional,
                dep.peer
            ));
        }
    }
    entries.sort_unstable();

    let mut hasher = Sha256::new();
    hasher.update(b"megagate-web-resolution-v1\0");
    hasher.update(registry_url.trim_end_matches('/').as_bytes());
    hasher.update(b"\0");
    for entry in entries {
        hasher.update(entry.as_bytes());
        hasher.update(b"\0");
    }
    format!("{:x}", hasher.finalize())
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
    fn ecosystem(&self) -> mg_types::ecosystem::Ecosystem {
        mg_types::ecosystem::Ecosystem::Web
    }
    fn can_handle(&self, project_root: &Path) -> bool {
        project_root.join("package.json").exists()
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let pkg_path = project_root.join("package.json");
        if !pkg_path.exists() {
            return Err(mg_types::MgError::Other(format!(
                "No package.json in '{}'. Run 'mg init --template web' first.",
                project_root.display()
            )));
        }
        const MAX_MANIFEST_SIZE: u64 = 10 * 1024 * 1024; // 10MB
        let metadata = std::fs::metadata(&pkg_path)?;
        if metadata.len() > MAX_MANIFEST_SIZE {
            return Err(mg_types::MgError::Other(format!(
                "package.json is too large ({} bytes, max {})",
                metadata.len(),
                MAX_MANIFEST_SIZE
            )));
        }
        let pkg_json: PackageJson = serde_json::from_str(&std::fs::read_to_string(&pkg_path)?)?;
        let mut manifest = Manifest::new(&pkg_json.name, mg_types::ecosystem::Ecosystem::Web);
        manifest.version = Some(Version::parse(&pkg_json.version).map_err(|_| {
            mg_types::MgError::Other(format!(
                "invalid version '{}' in package.json",
                pkg_json.version
            ))
        })?);
        let parse_deps = |map: Option<std::collections::HashMap<String, String>>| -> MgResult<Vec<mg_types::DependencySpec>> {
            match map {
                Some(deps) => {
                    let mut out = Vec::with_capacity(deps.len());
                    for (name, range) in deps {
                        if is_workspace_protocol_range(&range) {
                            continue;
                        }
                        let pn = PackageName::new(name)?;
                        let vr = VersionRange::parse(&range)?;
                        out.push(mg_types::DependencySpec::new(pn, vr));
                    }
                    Ok(out)
                }
                None => Ok(vec![]),
            }
        };
        manifest.dependencies = parse_deps(pkg_json.dependencies)?;
        manifest.dev_dependencies = parse_deps(pkg_json.dev_dependencies)?;
        manifest.peer_dependencies = parse_deps(pkg_json.peer_dependencies)?;
        manifest.optional_dependencies = parse_deps(pkg_json.optional_dependencies)?;
        Ok(manifest)
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        let to_map =
            |deps: &[mg_types::DependencySpec]| -> std::collections::HashMap<String, String> {
                deps.iter()
                    .map(|d| (d.name.as_str().to_string(), d.range.to_string()))
                    .collect()
            };
        let pkg_path = project_root.join("package.json");
        let fallback_version = manifest
            .version
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0.1.0".to_string());
        let existing = PackageJson::load(&pkg_path)
            .unwrap_or_else(|_| PackageJson::new(manifest.name.clone(), fallback_version));
        let pkg = PackageJson {
            name: manifest.name.clone(),
            version: manifest
                .version
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or(existing.version),
            description: existing.description,
            dependencies: if manifest.dependencies.is_empty() {
                None
            } else {
                Some(to_map(&manifest.dependencies))
            },
            dev_dependencies: if manifest.dev_dependencies.is_empty() {
                None
            } else {
                Some(to_map(&manifest.dev_dependencies))
            },
            peer_dependencies: if manifest.peer_dependencies.is_empty() {
                None
            } else {
                Some(to_map(&manifest.peer_dependencies))
            },
            optional_dependencies: if manifest.optional_dependencies.is_empty() {
                None
            } else {
                Some(to_map(&manifest.optional_dependencies))
            },
            extra: existing.extra,
        };
        pkg.save(&pkg_path)?;
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        let wanted: Vec<(PackageName, String)> = manifest
            .all_dependencies()
            .map(|d| (d.name.clone(), d.range.to_string()))
            .collect();
        if wanted.is_empty() {
            return Ok(ResolvedGraph::empty());
        }
        let resolution_cache_key = self
            .shared_cache
            .as_ref()
            .map(|_| manifest_resolution_cache_key(manifest, &self.registry_url));
        if let (Some(shared_cache), Some(key)) =
            (self.shared_cache.as_ref(), resolution_cache_key.as_deref())
        {
            if let Some(graph) = shared_cache.read_resolution(key, &self.registry_url)? {
                return Ok(graph);
            }
        }

        let result = self
            .resolver
            .solve(&wanted)
            .await
            .map_err(|e| mg_types::MgError::DependencyConflict(e.message))?;
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
            .map_err(|err| mg_types::MgError::Network(err.to_string()))?;
        let resolution_index: std::collections::HashMap<
            String,
            Vec<&mg_resolver::solver::Resolution>,
        > = result.resolutions.iter().fold(
            std::collections::HashMap::new(),
            |mut acc, resolution| {
                acc.entry(resolution.package_id.name_str().to_string())
                    .or_default()
                    .push(resolution);
                acc
            },
        );
        let packages = result
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
                            resolution_index
                                .get(name)
                                .and_then(|candidates| {
                                    candidates
                                        .iter()
                                        .filter(|candidate| constraint.matches(&candidate.version))
                                        .max_by(|left, right| left.version.cmp(&right.version))
                                })
                                .map(|candidate| candidate.package_id.clone())
                        })
                        .collect(),
                    direct: is_direct,
                    dev: is_dev,
                }
            })
            .collect();
        let graph = ResolvedGraph { packages };
        if let (Some(shared_cache), Some(key)) =
            (self.shared_cache.as_ref(), resolution_cache_key.as_deref())
        {
            let _ = shared_cache.write_resolution(key, &self.registry_url, &graph);
        }
        Ok(graph)
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        let reg = native::npm_registry::NpmRegistry::new(&self.registry_url);
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
                mg_types::MgError::Network(format!(
                    "download failed for '{}': {}",
                    pkg.id.name_str(),
                    e
                ))
            })?;
            if let Some(ref store) = self.store {
                store
                    .import_bytes(&bytes)
                    .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
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
        let start = std::time::Instant::now();
        let mut profile = InstallProfile::from_env();
        let registry = native::npm_registry::NpmRegistry::new(&self.registry_url);
        let store_root = project_cache_dir(project_root);
        let layout = Layout::new(store_root);
        std::fs::create_dir_all(layout.root())?;
        std::fs::create_dir_all(layout.temp_dir())?;

        let cache = PackageCache::new(layout.cache_dir())
            .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
        let shared_cache = self.shared_cache.clone();
        let database = Database::open(&layout.db_path())
            .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
        let default_store = ContentStore::new(layout.cas_dir())
            .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
        let store = self.store.as_ref().unwrap_or(&default_store);
        let node_modules = project_root.join("node_modules");
        std::fs::create_dir_all(&node_modules)?;
        let mut summary = InstallSummary::default();
        let thread_id_hash = {
            let tid = std::thread::current().id();
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&tid, &mut hasher);
            std::hash::Hasher::finish(&hasher)
        };

        let staging_root = layout.temp_dir().join(format!(
            "install-stage-{}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            thread_id_hash
        ));
        let staged_node_modules = staging_root.join("node_modules");
        std::fs::create_dir_all(&staged_node_modules)?;
        let root_packages = select_root_packages(graph);
        let root_package_versions: std::collections::HashMap<String, PackageId> = root_packages
            .iter()
            .map(|pkg| (pkg.id.name_str().to_string(), pkg.id.clone()))
            .collect();
        let package_map: std::collections::HashMap<PackageId, &ResolvedPackage> = graph
            .packages
            .iter()
            .map(|pkg| (pkg.id.clone(), pkg))
            .collect();
        let mut packages_with_scripts: Vec<std::path::PathBuf> = Vec::new();

        let already_materialized: std::collections::HashSet<PackageId> = root_packages
            .iter()
            .filter(|pkg| {
                installed_package_matches(&node_modules.join(pkg.id.name().as_str()), &pkg.id)
            })
            .map(|pkg| pkg.id.clone())
            .collect();
        let shared_package_cache_for_install = shared_cache
            .as_ref()
            .and_then(|shared| shared.package_cache().ok());
        let local_has_seeded_tarballs = graph
            .packages
            .iter()
            .any(|pkg| cache.contains_tarball(&pkg.id));
        let use_shared_primary =
            !local_has_seeded_tarballs && shared_package_cache_for_install.is_some();
        let active_package_cache = if use_shared_primary {
            shared_package_cache_for_install
                .as_ref()
                .expect("shared package cache checked above")
        } else {
            &cache
        };
        let secondary_shared_cache = if use_shared_primary {
            None
        } else {
            shared_cache.as_ref()
        };

        write_web_lockfile_with_state(project_root, graph, "installing")?;
        summary.bytes_from_cache += prefetch_tarballs(
            graph,
            &already_materialized,
            active_package_cache,
            secondary_shared_cache,
            &registry,
        )
        .await?;
        profile.mark("prefetch_tarballs", start);

        if opts.legacy_flat {
            let mut extracted_roots = std::collections::HashMap::new();
            profile.mark("prepare_extracted_roots", start);

            for pkg in &root_packages {
                let final_dir = node_modules.join(pkg.id.name().as_str());
                if installed_package_matches(&final_dir, &pkg.id) {
                    database
                        .insert_package(
                            &pkg.id,
                            if pkg.integrity.is_empty() {
                                None
                            } else {
                                Some(pkg.integrity.as_str())
                            },
                        )
                        .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
                    summary.added.push(pkg.id.clone());
                    continue;
                }

                let package_root = match extracted_root_for(
                    &mut extracted_roots,
                    &layout,
                    store,
                    shared_cache.as_ref(),
                    active_package_cache,
                    pkg,
                ) {
                    Ok(root) => root,
                    Err(err) => {
                        if staging_root.exists() {
                            let _ = std::fs::remove_dir_all(&staging_root);
                        }
                        return Err(err);
                    }
                };
                let materialized_dir = staged_node_modules.join(pkg.id.name().as_str());
                if materialized_dir.exists() {
                    std::fs::remove_dir_all(&materialized_dir)?;
                }
                if let Err(err) = hardlink_tree(package_root.as_path(), &materialized_dir) {
                    if staging_root.exists() {
                        let _ = std::fs::remove_dir_all(&staging_root);
                    }
                    return Err(err);
                }
                if let Err(err) = database
                    .insert_package(
                        &pkg.id,
                        if pkg.integrity.is_empty() {
                            None
                        } else {
                            Some(pkg.integrity.as_str())
                        },
                    )
                    .map_err(|e| mg_types::MgError::Store(e.to_string()))
                {
                    if staging_root.exists() {
                        let _ = std::fs::remove_dir_all(&staging_root);
                    }
                    return Err(err);
                }
                summary.added.push(pkg.id.clone());
            }

            for pkg in &root_packages {
                let staged_dir = staged_node_modules.join(pkg.id.name().as_str());
                let final_dir = node_modules.join(pkg.id.name().as_str());
                if !staged_dir.exists() {
                    continue;
                }
                if let Some(parent) = final_dir.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        mg_types::MgError::Other(format!(
                            "failed to create parent '{}' for '{}': {}",
                            parent.display(),
                            pkg.id.name_str(),
                            err
                        ))
                    })?;
                }
                if final_dir.exists() {
                    std::fs::remove_dir_all(&final_dir).map_err(|err| {
                        mg_types::MgError::Other(format!(
                            "failed to remove existing install dir '{}' for '{}': {}",
                            final_dir.display(),
                            pkg.id.name_str(),
                            err
                        ))
                    })?;
                }
                std::fs::rename(&staged_dir, &final_dir).map_err(|err| {
                    mg_types::MgError::Other(format!(
                        "failed to promote staged package '{}' from '{}' to '{}': {}",
                        pkg.id.name_str(),
                        staged_dir.display(),
                        final_dir.display(),
                        err
                    ))
                })?;
            }
        } else {
            for pkg in &root_packages {
                database
                    .insert_package(
                        &pkg.id,
                        if pkg.integrity.is_empty() {
                            None
                        } else {
                            Some(pkg.integrity.as_str())
                        },
                    )
                    .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
                summary.added.push(pkg.id.clone());
            }
        }
        profile.mark("materialize_root_packages", start);
        if opts.legacy_flat {
            for pkg in &root_packages {
                let package_dir = node_modules.join(pkg.id.name().as_str());
                reset_nested_node_modules(&package_dir)?;
                packages_with_scripts.push(package_dir.clone());
                let mut visiting = std::collections::HashSet::new();
                let mut extracted_roots = std::collections::HashMap::new();
                materialize_nested_dependencies(
                    &package_dir,
                    pkg,
                    &package_map,
                    &root_package_versions,
                    &layout,
                    store,
                    shared_cache.as_ref(),
                    active_package_cache,
                    &mut extracted_roots,
                    &mut visiting,
                    0,
                    &mut packages_with_scripts,
                )?;
            }
        } else {
            let extracted_roots = parallel_extract_packages(
                graph,
                &layout,
                store,
                shared_cache.as_ref(),
                active_package_cache,
            )
            .await?;
            profile.mark("prepare_extracted_roots", start);
            materialize_strict_layout(
                project_root,
                &node_modules,
                &staged_node_modules,
                graph,
                &package_map,
                &root_packages,
                &layout,
                store,
                shared_cache.as_ref(),
                active_package_cache,
                &mut packages_with_scripts,
                &extracted_roots,
            )?;
        }
        profile.mark("materialize_dependency_graph", start);
        prune_root_install_dirs(&node_modules, &root_package_versions)?;
        profile.mark("prune_root_install_dirs", start);
        if staging_root.exists() {
            std::fs::remove_dir_all(&staging_root).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to clean staging root '{}': {}",
                    staging_root.display(),
                    err
                ))
            })?;
        }
        rebuild_bin_links(&node_modules, &root_packages)?;
        profile.mark("rebuild_bin_links", start);

        write_web_lockfile_with_state(project_root, graph, "locked")?;
        profile.mark("write_lockfile", start);

        if !opts.ignore_scripts {
            use lifecycle::LifecycleRunner;
            use std::sync::Arc;
            use tokio::sync::Semaphore;
            use tokio::task::JoinSet;

            // Filter packages that actually have lifecycle scripts
            let mut scripted_packages = Vec::new();
            for pkg_dir in &packages_with_scripts {
                let package_json = pkg_dir.join("package.json");
                if package_json.exists() {
                    if let Ok(contents) = std::fs::read_to_string(&package_json) {
                        if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents) {
                            let has_scripts = manifest
                                .get("scripts")
                                .and_then(|s| s.as_object())
                                .map(|scripts| {
                                    scripts.contains_key("preinstall")
                                        || scripts.contains_key("install")
                                        || scripts.contains_key("postinstall")
                                })
                                .unwrap_or(false);
                            if has_scripts {
                                scripted_packages.push(pkg_dir.clone());
                            }
                        }
                    }
                }
            }

            // Run lifecycle scripts concurrently with a semaphore (max 8 parallel)
            let semaphore = Arc::new(Semaphore::new(8));
            let mut join_set = JoinSet::new();

            for pkg_dir in scripted_packages {
                let project_root = project_root.to_path_buf();
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                join_set.spawn(async move {
                    let _permit = permit;
                    LifecycleRunner::run_scripts(&pkg_dir, &project_root)
                });
            }

            while let Some(result) = join_set.join_next().await {
                if let Err(e) = result {
                    eprintln!("[megagate] warning: lifecycle script task panicked: {}", e);
                } else if let Err(e) = result.unwrap() {
                    eprintln!("[megagate] warning: lifecycle script error: {}", e);
                }
            }
        }
        profile.mark("lifecycle_scripts", start);

        summary.duration_ms = start.elapsed().as_millis() as u64;
        profile.flush(summary.duration_ms);
        Ok(summary)
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
    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        self.base_remove(project_root, name).await
    }
    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let manifest = self.parse_manifest(project_root).await?;
        let lockfile = read_web_lockfile_checked(project_root)?;
        let node_modules = project_root.join("node_modules");
        let mut packages = Vec::new();

        for (label, deps) in manifest.dep_groups() {
            let is_dev = label == "devDependencies";
            for dep in deps {
                let path = node_modules.join(dep.name.as_str());
                if !path.exists() {
                    continue;
                }

                let version = installed_package_version(&path)
                    .or_else(|| {
                        lockfile.as_ref().and_then(|lock| {
                            lock.packages
                                .iter()
                                .find(|pkg| pkg.name == dep.name.as_str())
                                .and_then(|pkg| Version::parse(&pkg.version).ok())
                        })
                    })
                    .unwrap_or_else(|| Version::new(0, 0, 0));

                let integrity = lockfile.as_ref().and_then(|lock| {
                    lock.packages
                        .iter()
                        .find(|pkg| pkg.name == dep.name.as_str())
                        .and_then(|pkg| pkg.integrity.clone())
                });
                let is_direct = lockfile
                    .as_ref()
                    .map(|lock| {
                        lock.packages
                            .iter()
                            .find(|pkg| pkg.name == dep.name.as_str())
                            .map(|pkg| pkg.direct)
                            .unwrap_or(true)
                    })
                    .unwrap_or(true);

                packages.push(InstalledPackage {
                    id: PackageId::new(dep.name.clone(), version),
                    path,
                    integrity,
                    is_direct,
                    is_dev,
                });
            }
        }

        if packages.is_empty() {
            self.base_list(project_root).await
        } else {
            Ok(packages)
        }
    }
    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        let mut manifest = self.parse_manifest(project_root).await?;
        let registry = native::npm_registry::NpmRegistry::new(&self.registry_url);
        let lockfile = read_web_lockfile_checked(project_root)?;
        let mut updated = Vec::new();

        for deps in [
            &mut manifest.dependencies,
            &mut manifest.dev_dependencies,
            &mut manifest.peer_dependencies,
            &mut manifest.optional_dependencies,
        ] {
            for dep in deps.iter_mut() {
                if let Some(selected) = name {
                    if dep.name != *selected {
                        continue;
                    }
                }

                let latest = self.latest_version_string(&dep.name, &registry).await?;
                let latest_version = Version::parse(&latest)?;
                if dep.range.matches(&latest_version) {
                    continue;
                }

                let from_version = lockfile
                    .as_ref()
                    .and_then(|lock| {
                        lock.packages
                            .iter()
                            .find(|pkg| pkg.direct && pkg.name == dep.name.as_str())
                            .map(|pkg| pkg.version.clone())
                    })
                    .unwrap_or_else(|| dep.range.to_string());

                dep.range = Self::preferred_saved_range(&dep.range, &latest)?;
                updated.push(UpdatedPackage {
                    name: dep.name.as_str().to_string(),
                    from_version,
                    to_version: latest,
                });
            }
        }

        if !updated.is_empty() {
            self.write_manifest(project_root, &manifest).await?;
        }

        Ok(updated)
    }
    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        use mg_types::adapter::{Vulnerability, VulnerabilitySeverity};

        let lockfile = match read_web_lockfile_checked(project_root)? {
            Some(lock) => lock,
            None => return Ok(AuditReport::clean(0)),
        };

        if lockfile.packages.is_empty() {
            return Ok(AuditReport::clean(0));
        }

        // Build the bulk-advisory request body for npm registry
        // POST https://registry.npmjs.org/-/npm/v1/security/advisories/bulk
        // Body: { "package@version": ["dependency_range"], ... }
        let mut body = serde_json::Map::new();
        for pkg in &lockfile.packages {
            let key = format!("{}", pkg.name);
            let version_entry = serde_json::json!([pkg.version.clone()]);
            body.insert(key, version_entry);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(format!("megagate/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| mg_types::MgError::Network(format!("audit client error: {e}")))?;

        let response = client
            .post("https://registry.npmjs.org/-/npm/v1/security/advisories/bulk")
            .json(&body)
            .send()
            .await
            .map_err(|e| mg_types::MgError::Network(format!("audit request failed: {e}")))?;

        let package_count = lockfile.packages.len();

        if !response.status().is_success() {
            return Err(mg_types::MgError::Network(format!(
                "audit API returned {}",
                response.status()
            )));
        }

        let advisories: serde_json::Value = response
            .json()
            .await
            .map_err(|e| mg_types::MgError::Other(format!("audit response parse error: {e}")))?;

        let mut vulnerabilities = Vec::new();
        if let Some(map) = advisories.as_object() {
            for (pkg_name, advisory_list) in map {
                if let Some(advisories_arr) = advisory_list.as_array() {
                    for advisory in advisories_arr {
                        let title = advisory["title"]
                            .as_str()
                            .unwrap_or("Unknown vulnerability")
                            .to_string();
                        let severity_str =
                            advisory["severity"].as_str().unwrap_or("info").to_string();
                        let cve = advisory["cves"]
                            .as_array()
                            .and_then(|cves| cves.first())
                            .and_then(|c| c.as_str())
                            .unwrap_or("CVE-UNKNOWN")
                            .to_string();
                        let patched = advisory["vulnerable_versions"]
                            .as_str()
                            .map(|s| s.to_string());
                        let url = advisory["url"].as_str().map(|s| s.to_string());
                        let version = advisory["findings"]
                            .as_array()
                            .and_then(|f| f.first())
                            .and_then(|f| f["version"].as_str())
                            .unwrap_or("unknown")
                            .to_string();

                        let pkg_name_parsed = mg_types::PackageName::new(pkg_name.clone())
                            .unwrap_or_else(|_| {
                                // Fallback: use raw string
                                mg_types::PackageName::new("unknown").unwrap()
                            });
                        let ver = mg_types::Version::parse(&version)
                            .unwrap_or_else(|_| mg_types::Version::parse("0.0.0").unwrap());

                        vulnerabilities.push(Vulnerability {
                            package: mg_types::PackageId::new(pkg_name_parsed, ver),
                            title,
                            severity: severity_str.clone(),
                            cve,
                            severity_level: VulnerabilitySeverity::from_str(&severity_str),
                            patched_versions: patched,
                            url,
                        });
                    }
                }
            }
        }

        let vuln_count = vulnerabilities.len();
        Ok(AuditReport {
            packages_audited: package_count,
            vulnerability_count: vuln_count,
            vulnerabilities,
        })
    }
}

fn is_workspace_protocol_range(range: &str) -> bool {
    range.trim().starts_with("workspace:")
}

struct NpmDependencyProvider {
    registry: native::npm_registry::NpmRegistry,
    metadata_cache: DashMap<String, native::npm_registry::PackageMetadata>,
    metadata_locks: DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    registry_cache: RegistryCache,
    shared_cache: Option<SharedWebCache>,
    alias_targets: DashMap<String, PackageName>,
}
impl NpmDependencyProvider {
    fn new(url: &str, shared_cache: Option<SharedWebCache>) -> Self {
        Self {
            registry: native::npm_registry::NpmRegistry::new(url),
            metadata_cache: DashMap::new(),
            metadata_locks: DashMap::new(),
            registry_cache: RegistryCache::new(),
            shared_cache,
            alias_targets: DashMap::new(),
        }
    }

    async fn metadata(
        &self,
        package: &PackageName,
    ) -> Result<native::npm_registry::PackageMetadata, DependencyError> {
        let source_package = self.source_package_name(package);
        if let Some(cached) = self.metadata_cache.get(source_package.as_str()) {
            return Ok(cached.clone());
        }
        let lock = self
            .metadata_locks
            .entry(source_package.as_str().to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        if let Some(cached) = self.metadata_cache.get(source_package.as_str()) {
            return Ok(cached.clone());
        }
        let meta = load_metadata_with_fallback(
            &source_package,
            &self.registry,
            self.shared_cache.as_ref(),
        )
        .await?;
        self.metadata_cache
            .insert(source_package.as_str().to_string(), meta.clone());
        Ok(meta)
    }

    fn source_package_name(&self, package: &PackageName) -> PackageName {
        self.alias_targets
            .get(package.as_str())
            .map(|entry| entry.clone())
            .unwrap_or_else(|| package.clone())
    }

    fn record_alias_target(&self, alias: &PackageName, target: &PackageName) {
        self.alias_targets
            .insert(alias.as_str().to_string(), target.clone());
    }

    fn parse_alias_spec(spec: &str) -> Option<(String, String)> {
        let remainder = spec.strip_prefix("npm:")?;
        let at = remainder.rfind('@')?;
        if at == 0 {
            return None;
        }
        let (package, range) = remainder.split_at(at);
        let range = range.strip_prefix('@')?;
        if package.trim().is_empty() || range.trim().is_empty() {
            return None;
        }
        Some((package.to_string(), range.to_string()))
    }

    fn collect_resolved_deps(
        &self,
        deps: Option<&std::collections::HashMap<String, String>>,
        optional: bool,
    ) -> Vec<ResolvedDep> {
        deps.into_iter()
            .flat_map(|deps| deps.iter())
            .filter_map(|(name, spec)| {
                let alias = PackageName::new(name).ok()?;
                if let Some((target, range)) = Self::parse_alias_spec(spec) {
                    let target = PackageName::new(target).ok()?;
                    self.record_alias_target(&alias, &target);
                    Some(ResolvedDep {
                        package: alias,
                        spec: range,
                        optional,
                        peer: false,
                    })
                } else {
                    Some(ResolvedDep {
                        package: alias,
                        spec: spec.clone(),
                        optional,
                        peer: false,
                    })
                }
            })
            .collect()
    }

    async fn prefetch_resolution_metadata(
        &self,
        names: &[PackageName],
    ) -> Result<
        std::collections::HashMap<String, native::npm_registry::PackageMetadata>,
        DependencyError,
    > {
        let mut results = std::collections::HashMap::new();
        let mut seen = std::collections::HashSet::new();
        let mut futures = Vec::new();

        for alias_name in names {
            if !seen.insert(alias_name.as_str().to_string()) {
                continue;
            }
            let alias_name = alias_name.clone();
            let source_name = self.source_package_name(&alias_name);
            if let Some(metadata) = self.metadata_cache.get(source_name.as_str()) {
                results.insert(alias_name.as_str().to_string(), metadata.clone());
                continue;
            }
            futures.push(async move {
                let metadata = self.metadata(&alias_name).await?;
                Ok::<_, DependencyError>((alias_name.as_str().to_string(), metadata))
            });
        }

        for fetched in join_all(futures).await {
            let (name, metadata) = fetched?;
            results.insert(name, metadata);
        }

        Ok(results)
    }

    fn version_key(package_id: &PackageId) -> String {
        format!("{}@{}", package_id.name_str(), package_id.version())
    }

    fn current_npm_os() -> &'static str {
        match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "win32",
            other => other,
        }
    }

    fn current_npm_cpu() -> &'static str {
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            "x86" => "ia32",
            "powerpc64" => "ppc64",
            "loongarch64" => "loong64",
            other => other,
        }
    }

    fn platform_matches(rules: Option<&[String]>, current: &str) -> bool {
        let Some(rules) = rules else {
            return true;
        };
        if rules.is_empty() {
            return true;
        }

        let mut positive = Vec::new();
        let mut negative = Vec::new();
        for rule in rules {
            if let Some(stripped) = rule.strip_prefix('!') {
                negative.push(stripped);
            } else {
                positive.push(rule.as_str());
            }
        }

        if negative.contains(&current) {
            return false;
        }
        if positive.is_empty() {
            true
        } else {
            positive.contains(&current)
        }
    }

    fn version_supported(info: &native::npm_registry::VersionInfo) -> bool {
        Self::platform_matches(info.os.as_deref(), Self::current_npm_os())
            && Self::platform_matches(info.cpu.as_deref(), Self::current_npm_cpu())
    }

    fn known_optional_native_binary_supported(package: &PackageName) -> Option<bool> {
        let name = package.as_str();
        let os = Self::current_npm_os();
        let cpu = Self::current_npm_cpu();
        let expected = format!("{os}-{cpu}");

        let target = name
            .strip_prefix("@esbuild/")
            .or_else(|| name.strip_prefix("@swc/core-"))
            .or_else(|| name.strip_prefix("@rollup/rollup-"))
            .or_else(|| name.strip_prefix("@tailwindcss/oxide-"))
            .or_else(|| name.strip_prefix("lightningcss-"))
            .or_else(|| name.strip_prefix("@parcel/watcher-"))?;

        Some(target.starts_with(&expected))
    }

    fn select_best_version(
        versions: &[Version],
        spec: &str,
    ) -> Result<Option<Version>, DependencyError> {
        let constraint = VersionRange::parse(spec)
            .map_err(|e| DependencyError(format!("invalid spec '{}': {}", spec, e)))?;
        let mut matches: Vec<Version> = versions
            .iter()
            .filter(|v| constraint.matches(v))
            .cloned()
            .collect();
        if matches.is_empty() {
            return Ok(None);
        }

        let allows_prerelease = spec.contains('-');
        if !allows_prerelease {
            let stable = matches.iter().filter(|v| v.pre.is_none()).cloned().max();
            if stable.is_some() {
                return Ok(stable);
            }
        }

        matches.sort();
        Ok(matches.into_iter().max())
    }
}
#[async_trait]
impl DependencyProvider for NpmDependencyProvider {
    async fn get_versions(&self, package: &PackageName) -> Result<Vec<Version>, DependencyError> {
        if let Some(cached) = self.registry_cache.get_versions(package.as_str()) {
            return Ok(cached);
        }
        let meta = self.metadata(package).await?;
        let mut v: Vec<Version> = meta
            .versions
            .keys()
            .filter_map(|k| Version::parse(k).ok())
            .collect();
        v.sort();
        self.registry_cache
            .insert_versions(package.as_str().to_string(), v.clone());
        Ok(v)
    }
    async fn get_dependencies(
        &self,
        package_id: &PackageId,
    ) -> Result<Vec<ResolvedDep>, DependencyError> {
        let cache_key = Self::version_key(package_id);
        if let Some(cached) = self.registry_cache.get_deps(&cache_key) {
            return Ok(cached);
        }
        let meta = self.metadata(package_id.name()).await?;
        let deps: Vec<ResolvedDep> = meta
            .versions
            .get(&package_id.version().to_string())
            .map(|v| {
                let mut collected = self.collect_resolved_deps(v.dependencies.as_ref(), false);
                collected
                    .extend(self.collect_resolved_deps(v.optional_dependencies.as_ref(), true));
                collected
            })
            .unwrap_or_default();
        self.registry_cache.insert_deps(cache_key, deps.clone());
        Ok(deps)
    }

    async fn should_enqueue(&self, dep: &ResolvedDep) -> Result<bool, DependencyError> {
        if !dep.optional {
            return Ok(true);
        }

        if let Some(supported) = Self::known_optional_native_binary_supported(&dep.package) {
            return Ok(supported);
        }

        let versions = self.get_versions(&dep.package).await?;
        let Some(selected) = Self::select_best_version(&versions, &dep.spec)? else {
            return Ok(!dep.optional);
        };
        let meta = self.metadata(&dep.package).await?;
        let Some(info) = meta.versions.get(&selected.to_string()) else {
            return Ok(!dep.optional);
        };

        Ok(Self::version_supported(info))
    }

    async fn prefetch_versions(
        &self,
        packages: &[PackageName],
    ) -> Result<Vec<(PackageName, Vec<Version>)>, DependencyError> {
        let mut results = Vec::with_capacity(packages.len());
        let mut futures = Vec::new();

        for package in packages {
            if let Some(cached) = self.registry_cache.get_versions(package.as_str()) {
                results.push((package.clone(), cached));
                continue;
            }

            let package_key = self.source_package_name(package).as_str().to_string();
            if let Some(metadata) = self.metadata_cache.get(&package_key) {
                let mut versions: Vec<Version> = metadata
                    .versions
                    .keys()
                    .filter_map(|k| Version::parse(k).ok())
                    .collect();
                versions.sort();
                self.registry_cache
                    .insert_versions(package.as_str().to_string(), versions.clone());
                results.push((package.clone(), versions));
                continue;
            }
            let package_name = package.clone();
            futures.push(async move {
                let metadata = self.metadata(&package_name).await?;
                Ok::<_, DependencyError>((package_name, metadata))
            });
        }

        for fetched in join_all(futures).await {
            let (package, metadata) = fetched?;
            let source_key = self.source_package_name(&package).as_str().to_string();
            self.metadata_cache.insert(source_key, metadata.clone());
            let mut versions: Vec<Version> = metadata
                .versions
                .keys()
                .filter_map(|k| Version::parse(k).ok())
                .collect();
            versions.sort();
            self.registry_cache
                .insert_versions(package.as_str().to_string(), versions.clone());
            results.push((package, versions));
        }

        Ok(results)
    }

    async fn prefetch_dependencies(
        &self,
        ids: &[PackageId],
    ) -> Result<Vec<(PackageId, Vec<ResolvedDep>)>, DependencyError> {
        let mut results = Vec::with_capacity(ids.len());
        let mut preloaded = Vec::new();

        for id in ids {
            let cache_key = Self::version_key(id);
            if let Some(cached) = self.registry_cache.get_deps(&cache_key) {
                results.push((id.clone(), cached));
            } else if let Some(meta) = self
                .metadata_cache
                .get(self.source_package_name(id.name()).as_str())
            {
                let deps = meta
                    .versions
                    .get(&id.version().to_string())
                    .map(|v| {
                        let mut collected =
                            self.collect_resolved_deps(v.dependencies.as_ref(), false);
                        collected.extend(
                            self.collect_resolved_deps(v.optional_dependencies.as_ref(), true),
                        );
                        collected
                    })
                    .unwrap_or_default();
                self.registry_cache.insert_deps(cache_key, deps.clone());
                results.push((id.clone(), deps));
            } else {
                preloaded.push(id.clone());
            }
        }

        if preloaded.is_empty() {
            return Ok(results);
        }

        let mut futures = Vec::new();

        for id in preloaded {
            let package_id = id.clone();
            futures.push(async move {
                let meta = self.metadata(package_id.name()).await?;
                Ok::<_, DependencyError>((package_id, meta))
            });
        }

        for fetched in join_all(futures).await {
            let (package_id, meta) = fetched?;
            let deps = meta
                .versions
                .get(&package_id.version().to_string())
                .map(|v| {
                    let mut collected = self.collect_resolved_deps(v.dependencies.as_ref(), false);
                    collected
                        .extend(self.collect_resolved_deps(v.optional_dependencies.as_ref(), true));
                    collected
                })
                .unwrap_or_default();
            self.registry_cache
                .insert_deps(Self::version_key(&package_id), deps.clone());
            results.push((package_id, deps));
        }

        Ok(results)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "devDependencies")]
    pub dev_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "peerDependencies")]
    pub peer_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "optionalDependencies"
    )]
    pub optional_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl PackageJson {
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            version,
            description: None,
            dependencies: None,
            dev_dependencies: None,
            peer_dependencies: None,
            optional_dependencies: None,
            extra: Map::new(),
        }
    }
    pub fn load(path: &Path) -> Result<Self, anyhow::Error> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }
    pub fn save(&self, path: &Path) -> Result<(), anyhow::Error> {
        let content = serde_json::to_string_pretty(self)?;
        atomic_write(path, content.as_bytes())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedWebCache {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedMetadataEnvelope {
    fetched_at: u64,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    stale_retry_after: Option<u64>,
    metadata: native::npm_registry::PackageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedResolutionEnvelope {
    cache_version: u32,
    registry_url: String,
    graph: ResolvedGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ExtractedPackageMarker {
    name: String,
    version: String,
    #[serde(default)]
    integrity: Option<String>,
    tarball_sha256: String,
}

#[derive(Debug, Clone)]
struct CachedMetadataRecord {
    fetched_at: u64,
    etag: Option<String>,
    stale_retry_after: Option<u64>,
    metadata: native::npm_registry::PackageMetadata,
}

impl SharedWebCache {
    fn discover() -> Option<Self> {
        if let Ok(path) = std::env::var("MEGAGATE_SHARED_CACHE_DIR") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                let candidate = Self {
                    root: PathBuf::from(trimmed),
                };
                if candidate.is_usable() {
                    candidate.maybe_prune_once_per_process();
                    return Some(candidate);
                }
                return None;
            }
        }

        dirs::cache_dir().and_then(|root| {
            let candidate = Self {
                root: root.join("megagate").join("web"),
            };
            if candidate.is_usable() {
                candidate.maybe_prune_once_per_process();
                Some(candidate)
            } else {
                None
            }
        })
    }

    fn is_usable(&self) -> bool {
        if std::fs::create_dir_all(&self.root).is_err() {
            return false;
        }

        let probe = self.root.join(".mg-write-probe");
        match std::fs::create_dir(&probe) {
            Ok(()) => {
                let _ = std::fs::remove_dir(&probe);
                true
            }
            Err(_) => false,
        }
    }

    fn package_cache(&self) -> anyhow::Result<PackageCache> {
        let layout = Layout::new(self.root.clone());
        std::fs::create_dir_all(layout.root())?;
        PackageCache::new(layout.cache_dir())
    }

    fn extracted_package_root(&self, pkg: &ResolvedPackage) -> PathBuf {
        shared_extracted_package_root(&self.root, pkg)
    }

    fn metadata_path(&self, package: &str) -> PathBuf {
        self.root
            .join("metadata")
            .join(package)
            .join("metadata.json")
    }

    fn resolution_path(&self, key: &str) -> PathBuf {
        self.root.join("resolutions").join(format!("{key}.json"))
    }

    fn read_resolution(&self, key: &str, registry_url: &str) -> MgResult<Option<ResolvedGraph>> {
        let path = self.resolution_path(key);
        if !path.exists() {
            return Ok(None);
        }

        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(_) => return Ok(None),
        };
        let envelope = match serde_json::from_str::<CachedResolutionEnvelope>(&contents) {
            Ok(envelope) => envelope,
            Err(_) => {
                let _ = std::fs::remove_file(&path);
                return Ok(None);
            }
        };
        if envelope.cache_version != 1 || envelope.registry_url != registry_url {
            return Ok(None);
        }
        Ok(Some(envelope.graph))
    }

    fn write_resolution(
        &self,
        key: &str,
        registry_url: &str,
        graph: &ResolvedGraph,
    ) -> MgResult<()> {
        let path = self.resolution_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec(&CachedResolutionEnvelope {
            cache_version: 1,
            registry_url: registry_url.to_string(),
            graph: graph.clone(),
        })?;
        atomic_write(&path, &payload)
    }

    fn read_metadata(
        &self,
        package: &str,
    ) -> Result<Option<CachedMetadataRecord>, DependencyError> {
        let path = self.metadata_path(package);
        if !path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&path).map_err(|e| {
            DependencyError(format!(
                "failed to read cached metadata for '{}': {}",
                package, e
            ))
        })?;
        if let Ok(envelope) = serde_json::from_str::<CachedMetadataEnvelope>(&contents) {
            return Ok(Some(CachedMetadataRecord {
                fetched_at: envelope.fetched_at,
                etag: envelope.etag.clone(),
                stale_retry_after: envelope.stale_retry_after,
                metadata: envelope.metadata,
            }));
        }

        let metadata = serde_json::from_str(&contents).map_err(|e| {
            DependencyError(format!(
                "failed to parse cached metadata for '{}': {}",
                package, e
            ))
        })?;
        Ok(Some(CachedMetadataRecord {
            fetched_at: 0,
            etag: None,
            stale_retry_after: None,
            metadata,
        }))
    }

    fn write_metadata(
        &self,
        package: &str,
        metadata: &native::npm_registry::PackageMetadata,
        etag: Option<String>,
    ) -> Result<(), DependencyError> {
        self.write_metadata_record(package, metadata, etag, current_unix_secs(), None)
    }

    fn write_metadata_record(
        &self,
        package: &str,
        metadata: &native::npm_registry::PackageMetadata,
        etag: Option<String>,
        fetched_at: u64,
        stale_retry_after: Option<u64>,
    ) -> Result<(), DependencyError> {
        let path = self.metadata_path(package);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DependencyError(format!(
                    "failed to create metadata cache dir for '{}': {}",
                    package, e
                ))
            })?;
        }
        let payload = serde_json::to_vec(&CachedMetadataEnvelope {
            fetched_at,
            etag,
            stale_retry_after,
            metadata: metadata.clone(),
        })
        .map_err(|e| {
            DependencyError(format!(
                "failed to serialize cached metadata for '{}': {}",
                package, e
            ))
        })?;
        atomic_write(&path, &payload).map_err(|e| {
            DependencyError(format!(
                "failed to write cached metadata for '{}': {}",
                package,
                e.to_string()
            ))
        })?;
        Ok(())
    }

    fn maybe_prune(&self) {
        let _ = prune_shared_cache_to_quota(&self.root, shared_cache_max_bytes());

        if !shared_cache_prune_due(&self.root) {
            return;
        }

        let max_age = std::time::Duration::from_secs(shared_cache_max_age_secs());
        let _ = prune_old_files_under(&self.root.join("cache"), max_age);
        let _ = prune_old_files_under(&self.root.join("resolutions"), max_age);
        let _ = prune_old_package_dirs_under(&self.root.join("packages"), max_age);
        let _ = prune_old_metadata_dirs_under(&self.root.join("metadata"), max_age);
        let _ = prune_shared_cache_to_quota(&self.root, shared_cache_max_bytes());
        let _ = write_shared_cache_prune_stamp(&self.root);
    }

    fn maybe_prune_once_per_process(&self) {
        static PRUNED_ROOTS: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
        let pruned_roots =
            PRUNED_ROOTS.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
        let mut guard = match pruned_roots.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if !guard.insert(self.root.clone()) {
            return;
        }
        drop(guard);
        self.maybe_prune();
    }
}

async fn load_metadata_with_fallback(
    package: &PackageName,
    registry: &native::npm_registry::NpmRegistry,
    shared_cache: Option<&SharedWebCache>,
) -> Result<native::npm_registry::PackageMetadata, DependencyError> {
    load_metadata_by_name_with_fallback(package.as_str(), registry, shared_cache).await
}

async fn load_metadata_by_name_with_fallback(
    package: &str,
    registry: &native::npm_registry::NpmRegistry,
    shared_cache: Option<&SharedWebCache>,
) -> Result<native::npm_registry::PackageMetadata, DependencyError> {
    let cached = if let Some(shared_cache) = shared_cache {
        shared_cache.read_metadata(package)?
    } else {
        None
    };

    if let Some(cached) = cached.as_ref() {
        if metadata_record_is_fresh(cached) {
            return Ok(cached.metadata.clone());
        }

        if metadata_record_retry_deferred(cached) && metadata_record_is_usable_stale(cached) {
            return Ok(cached.metadata.clone());
        }

        if let Some(etag) = &cached.etag {
            match registry
                .fetch_metadata_conditional(package, Some(etag))
                .await
            {
                Ok(None) => {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata(
                            package,
                            &cached.metadata,
                            Some(etag.clone()),
                        );
                    }
                    return Ok(cached.metadata.clone());
                }
                Ok(Some((metadata, new_etag))) => {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata(package, &metadata, Some(new_etag));
                    }
                    return Ok(metadata);
                }
                Err(_) => {
                    if !metadata_record_is_usable_stale(cached) {
                        return Err(DependencyError(format!(
                            "npm metadata refresh failed for '{}' and cached metadata is too old to reuse",
                            package
                        )));
                    }
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata_record(
                            package,
                            &cached.metadata,
                            Some(etag.clone()),
                            cached.fetched_at,
                            Some(next_stale_retry_after()),
                        );
                    }
                    return Ok(cached.metadata.clone());
                }
            }
        }
    }

    match registry.fetch_metadata_with_etag(package).await {
        Ok((metadata, etag)) => {
            if let Some(shared_cache) = shared_cache {
                let _ = shared_cache.write_metadata(package, &metadata, etag);
            }
            Ok(metadata)
        }
        Err(network_err) => {
            if let Some(cached) = cached {
                if !metadata_record_is_usable_stale(&cached) {
                    return Err(DependencyError(format!(
                        "npm metadata fetch failed for '{}' and cached metadata is too old to reuse: {}",
                        package, network_err
                    )));
                }
                if let Some(shared_cache) = shared_cache {
                    let _ = shared_cache.write_metadata_record(
                        package,
                        &cached.metadata,
                        cached.etag.clone(),
                        cached.fetched_at,
                        Some(next_stale_retry_after()),
                    );
                }
                return Ok(cached.metadata);
            }
            Err(DependencyError(format!(
                "npm metadata fetch failed for '{}': {}",
                package, network_err
            )))
        }
    }
}

async fn prefetch_tarballs(
    graph: &ResolvedGraph,
    skip: &std::collections::HashSet<PackageId>,
    cache: &PackageCache,
    shared_cache: Option<&SharedWebCache>,
    registry: &native::npm_registry::NpmRegistry,
) -> MgResult<u64> {
    enum PrefetchOutcome {
        CacheHit(u64),
        Downloaded(ResolvedPackage, Vec<u8>),
    }

    let mut bytes_from_cache = 0u64;
    let shared_package_cache = shared_cache
        .map(|shared| shared.package_cache())
        .transpose()
        .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
    let download_semaphore = Arc::new(tokio::sync::Semaphore::new(download_concurrency_limit()));
    let mut downloads = tokio::task::JoinSet::new();

    for pkg in &graph.packages {
        if skip.contains(&pkg.id) {
            continue;
        }
        let pkg_clone = pkg.clone();
        let local_cache = cache.clone();
        let shared_package_cache = shared_package_cache.clone();
        let download_semaphore = Arc::clone(&download_semaphore);
        let registry = native::npm_registry::NpmRegistry::new(registry.registry_url());
        downloads.spawn(async move {
            let prefetch_lock = tarball_prefetch_lock(&pkg_clone.id);
            let _guard = prefetch_lock.lock().await;

            if let Some(bytes) = local_cache
                .get_tarball(&pkg_clone.id)
                .map_err(|e| mg_types::MgError::Store(e.to_string()))?
            {
                if verify_tarball_integrity(&pkg_clone, &bytes).is_ok() {
                    return Ok::<_, mg_types::MgError>(PrefetchOutcome::CacheHit(
                        bytes.len() as u64
                    ));
                }
                let _ = std::fs::remove_file(local_cache.tarball_path(&pkg_clone.id));
            }

            if let Some(shared_package_cache) = shared_package_cache.as_ref() {
                if let Some(bytes) = shared_package_cache
                    .get_tarball(&pkg_clone.id)
                    .map_err(|e| mg_types::MgError::Store(e.to_string()))?
                {
                    if verify_tarball_integrity(&pkg_clone, &bytes).is_ok() {
                        local_cache
                            .cache_tarball_from_path(
                                &pkg_clone.id,
                                &shared_package_cache.tarball_path(&pkg_clone.id),
                            )
                            .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
                        return Ok::<_, mg_types::MgError>(PrefetchOutcome::CacheHit(
                            bytes.len() as u64
                        ));
                    }
                    let _ = std::fs::remove_file(shared_package_cache.tarball_path(&pkg_clone.id));
                }
            }

            let url = package_tarball_url(registry.registry_url(), &pkg_clone);
            let _permit = download_semaphore
                .acquire_owned()
                .await
                .map_err(|e| mg_types::MgError::Other(format!("download semaphore closed: {e}")))?;
            let bytes = registry.download_tarball(&url).await.map_err(|e| {
                mg_types::MgError::Network(format!(
                    "download failed for '{}': {}",
                    pkg_clone.id.name_str(),
                    e
                ))
            })?;
            Ok::<_, mg_types::MgError>(PrefetchOutcome::Downloaded(pkg_clone, bytes))
        });
    }

    while let Some(joined) = downloads.join_next().await {
        match joined
            .map_err(|e| mg_types::MgError::Other(format!("download task failed: {e}")))??
        {
            PrefetchOutcome::CacheHit(bytes) => {
                bytes_from_cache += bytes;
            }
            PrefetchOutcome::Downloaded(mut pkg, bytes) => {
                if pkg.integrity.is_empty() {
                    pkg.integrity = compute_tarball_integrity(&bytes);
                }
                verify_tarball_integrity(&pkg, &bytes)?;
                cache
                    .cache_tarball(&pkg.id, &bytes)
                    .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
                if let Some(shared_package_cache) = shared_package_cache.as_ref() {
                    let _ = shared_package_cache
                        .cache_tarball_from_path(&pkg.id, &cache.tarball_path(&pkg.id));
                }
            }
        }
    }

    Ok(bytes_from_cache)
}

fn metadata_record_is_fresh(record: &CachedMetadataRecord) -> bool {
    if record.fetched_at == 0 {
        return false;
    }
    current_unix_secs().saturating_sub(record.fetched_at) <= metadata_ttl_secs()
}

fn metadata_record_retry_deferred(record: &CachedMetadataRecord) -> bool {
    record
        .stale_retry_after
        .is_some_and(|retry_after| retry_after > current_unix_secs())
}

fn metadata_record_is_usable_stale(record: &CachedMetadataRecord) -> bool {
    if record.fetched_at == 0 {
        return true;
    }
    current_unix_secs().saturating_sub(record.fetched_at) <= metadata_max_stale_fallback_secs()
}

fn metadata_ttl_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_METADATA_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(6 * 60 * 60)
}

fn metadata_max_stale_fallback_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(7 * 24 * 60 * 60)
}

fn metadata_stale_retry_ttl_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_METADATA_STALE_RETRY_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(30)
}

fn shared_cache_prune_interval_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_CACHE_PRUNE_INTERVAL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(6 * 60 * 60)
}

fn download_concurrency_limit() -> usize {
    std::env::var("MEGAGATE_WEB_DOWNLOAD_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(24)
}

fn shared_cache_max_age_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_CACHE_MAX_AGE_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(7 * 24 * 60 * 60)
}

fn shared_cache_max_bytes() -> u64 {
    std::env::var("MEGAGATE_WEB_CACHE_MAX_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(2 * 1024 * 1024 * 1024)
}

fn strict_integrity_enforced() -> bool {
    let val = std::env::var("MEGAGATE_WEB_STRICT_INTEGRITY");
    !matches!(val.as_deref(), Ok("0" | "false" | "no" | "off"))
}

fn next_stale_retry_after() -> u64 {
    current_unix_secs().saturating_add(metadata_stale_retry_ttl_secs())
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn shared_cache_prune_stamp_path(root: &Path) -> PathBuf {
    root.join(".gc-stamp")
}

fn shared_cache_prune_due(root: &Path) -> bool {
    let stamp = shared_cache_prune_stamp_path(root);
    let Ok(metadata) = std::fs::metadata(&stamp) else {
        return true;
    };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    let Ok(elapsed) = modified.elapsed() else {
        return true;
    };
    elapsed >= std::time::Duration::from_secs(shared_cache_prune_interval_secs())
}

fn write_shared_cache_prune_stamp(root: &Path) -> MgResult<()> {
    let stamp = shared_cache_prune_stamp_path(root);
    let data = current_unix_secs().to_string();
    atomic_write(&stamp, data.as_bytes())?;
    Ok(())
}

fn prune_old_files_under(root: &Path, max_age: std::time::Duration) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut directories = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            directories.push(entry.path().to_path_buf());
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if path_is_older_than(entry.path(), max_age) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        remove_dir_if_empty(&dir);
    }
    Ok(())
}

fn prune_old_package_dirs_under(root: &Path, max_age: std::time::Duration) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut directories = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            directories.push(entry.path().to_path_buf());
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        let marker = dir.join(".megagate-package-root.json");
        let package_json = dir.join("package.json");
        if marker.exists() || package_json.exists() {
            if path_is_older_than(
                if marker.exists() {
                    &marker
                } else {
                    &package_json
                },
                max_age,
            ) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        } else {
            remove_dir_if_empty(&dir);
        }
    }
    Ok(())
}

fn prune_old_metadata_dirs_under(root: &Path, max_age: std::time::Duration) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to read metadata cache root '{}': {}",
            root.display(),
            err
        ))
    })? {
        let Ok(entry) = entry else {
            continue;
        };
        let dir = entry.path();
        let metadata_json = dir.join("metadata.json");
        if metadata_json.exists() && path_is_older_than(&metadata_json, max_age) {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum CachePruneEntryKind {
    File,
    Directory,
}

#[derive(Debug)]
struct CachePruneEntry {
    path: PathBuf,
    bytes: u64,
    modified: std::time::SystemTime,
    kind: CachePruneEntryKind,
}

fn prune_shared_cache_to_quota(root: &Path, max_bytes: u64) -> MgResult<()> {
    if max_bytes == 0 || !root.exists() {
        return Ok(());
    }

    let mut entries = Vec::new();
    collect_prunable_files(&root.join("cache"), &mut entries);
    collect_prunable_files(&root.join("metadata"), &mut entries);
    collect_prunable_files(&root.join("resolutions"), &mut entries);
    collect_prunable_package_dirs(&root.join("packages"), &mut entries);

    let mut total: u64 = entries.iter().map(|entry| entry.bytes).sum();
    if total <= max_bytes {
        return Ok(());
    }

    entries.sort_by_key(|entry| entry.modified);
    for entry in entries {
        if total <= max_bytes {
            break;
        }
        let removed = match entry.kind {
            CachePruneEntryKind::File => std::fs::remove_file(&entry.path).is_ok(),
            CachePruneEntryKind::Directory => std::fs::remove_dir_all(&entry.path).is_ok(),
        };
        if removed {
            total = total.saturating_sub(entry.bytes);
        }
    }

    cleanup_empty_dirs(&root.join("cache"));
    cleanup_empty_dirs(&root.join("metadata"));
    cleanup_empty_dirs(&root.join("resolutions"));
    cleanup_empty_dirs(&root.join("packages"));
    Ok(())
}

fn collect_prunable_files(root: &Path, entries: &mut Vec<CachePruneEntry>) {
    if !root.exists() {
        return;
    }
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        entries.push(CachePruneEntry {
            path: entry.path().to_path_buf(),
            bytes: metadata.len(),
            modified: metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
            kind: CachePruneEntryKind::File,
        });
    }
}

fn collect_prunable_package_dirs(root: &Path, entries: &mut Vec<CachePruneEntry>) {
    if !root.exists() {
        return;
    }
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        let marker = path.join(".megagate-package-root.json");
        if !marker.exists() {
            continue;
        }
        let modified = std::fs::metadata(&marker)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        entries.push(CachePruneEntry {
            path: path.to_path_buf(),
            bytes: directory_size(path),
            modified,
            kind: CachePruneEntryKind::Directory,
        });
    }
}

fn directory_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

fn cleanup_empty_dirs(root: &Path) {
    if !root.exists() {
        return;
    }
    let mut directories: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        remove_dir_if_empty(&dir);
    }
}

fn path_is_older_than(path: &Path, max_age: std::time::Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed >= max_age)
}

fn remove_dir_if_empty(path: &Path) {
    let _ = std::fs::remove_dir(path);
}

fn is_tarball_url_trusted(tarball_url: &str, registry_url: &str) -> bool {
    let Ok(tarball_parsed) = url::Url::parse(tarball_url) else {
        return false;
    };

    let Ok(registry_parsed) = url::Url::parse(registry_url) else {
        return false;
    };

    let Some(tarball_host) = tarball_parsed.host_str() else {
        return false;
    };

    let Some(registry_host) = registry_parsed.host_str() else {
        return false;
    };

    if tarball_host == registry_host {
        return true;
    }

    if registry_host == "registry.npmjs.org" {
        return tarball_host == "registry.npmjs.org"
            || tarball_host.ends_with(".npmjs.org")
            || tarball_host == "registry.yarnpkg.com";
    }

    if let Ok(allowed) = std::env::var("MEGAGATE_WEB_ALLOWED_TARBALL_HOSTS") {
        let allowed_hosts: Vec<&str> = allowed
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if allowed_hosts.iter().any(|&host| host == tarball_host) {
            return true;
        }
    }

    false
}

fn package_tarball_url(registry_url: &str, pkg: &ResolvedPackage) -> String {
    if !pkg.tarball_url.is_empty() {
        if !pkg.tarball_url.starts_with("https://")
            && !allow_insecure_loopback_url(&pkg.tarball_url)
        {
            eprintln!(
                "WARNING: Tarball URL for '{}' is not HTTPS, using registry fallback",
                pkg.id.name_str()
            );
            let registry = registry_url.trim_end_matches('/');
            let unscoped = pkg.id.name().unscoped();
            return format!(
                "{}/{}/-/{}-{}.tgz",
                registry,
                pkg.id.name_str(),
                unscoped,
                pkg.id.version()
            );
        }

        if !is_tarball_url_trusted(&pkg.tarball_url, registry_url) {
            eprintln!(
                "WARNING: Tarball URL for '{}' domain mismatch with registry, using registry fallback",
                pkg.id.name_str()
            );
            let registry = registry_url.trim_end_matches('/');
            let unscoped = pkg.id.name().unscoped();
            return format!(
                "{}/{}/-/{}-{}.tgz",
                registry,
                pkg.id.name_str(),
                unscoped,
                pkg.id.version()
            );
        }

        return pkg.tarball_url.clone();
    }

    let unscoped = pkg.id.name().unscoped();
    format!(
        "{}/{}/-/{}-{}.tgz",
        registry_url.trim_end_matches('/'),
        pkg.id.name_str(),
        unscoped,
        pkg.id.version()
    )
}

fn extracted_package_root(layout: &Layout, pkg: &ResolvedPackage) -> std::path::PathBuf {
    let safe_name = pkg.id.name_str().replace('/', "__").replace('@', "");
    layout
        .temp_dir()
        .join(format!("{}-{}", safe_name, pkg.id.version()))
}

fn local_extracted_package_root(layout: &Layout, pkg: &ResolvedPackage) -> PathBuf {
    shared_extracted_package_root(layout.root(), pkg)
}

fn shared_extracted_package_root(root: &Path, pkg: &ResolvedPackage) -> PathBuf {
    let safe_name = pkg.id.name_str().replace('/', "__").replace('@', "");
    let cache_key = extracted_package_cache_key(pkg);
    root.join("packages")
        .join(safe_name)
        .join(cache_key)
        .join("package")
}

fn extracted_package_root_lock(root: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<PathBuf, Arc<Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry(root.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn tarball_prefetch_lock(id: &PackageId) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let key = format!("{}@{}", id.name_str(), id.version());
    let mut guard = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

fn extracted_package_cache_key(pkg: &ResolvedPackage) -> String {
    if pkg.integrity.is_empty() {
        return pkg.id.version().to_string();
    }
    let fs_safe = pkg
        .integrity
        .replace('/', "_")
        .replace('+', "-")
        .replace('=', "");
    format!("{}-{}", pkg.id.version(), fs_safe)
}

fn compute_sha256_b64(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn compute_sha512_b64(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

fn verify_tarball_integrity(pkg: &ResolvedPackage, bytes: &[u8]) -> MgResult<()> {
    verify_sri_integrity(pkg, bytes)
}

fn compute_tarball_integrity(bytes: &[u8]) -> String {
    format!("sha512-{}", compute_sha512_b64(bytes))
}

fn verify_sri_integrity(pkg: &ResolvedPackage, bytes: &[u8]) -> MgResult<()> {
    if pkg.integrity.is_empty() {
        if strict_integrity_enforced() {
            return Err(mg_types::MgError::Other(format!(
                "strict integrity: '{}' has no SRI integrity field",
                pkg.id.name_str()
            )));
        }
        return Ok(());
    }

    let mut has_weak_algorithm = false;
    let mut has_strong_algorithm = false;

    for entry in pkg.integrity.split_whitespace() {
        let Some((algorithm, expected)) = entry.split_once('-') else {
            continue;
        };

        if matches!(algorithm, "sha1" | "md5") {
            has_weak_algorithm = true;
            if strict_integrity_enforced() {
                return Err(mg_types::MgError::Other(format!(
                    "strict integrity: '{}' uses weak hash algorithm '{}' (only sha256/sha512 allowed)",
                    pkg.id.name_str(),
                    algorithm
                )));
            }
            eprintln!(
                "WARNING: Package '{}' uses weak hash algorithm '{}', consider updating",
                pkg.id.name_str(),
                algorithm
            );
            continue;
        }

        let actual = match algorithm {
            "sha256" => {
                has_strong_algorithm = true;
                compute_sha256_b64(bytes)
            }
            "sha512" => {
                has_strong_algorithm = true;
                compute_sha512_b64(bytes)
            }
            _ => {
                eprintln!(
                    "WARNING: Package '{}' uses unknown hash algorithm '{}'",
                    pkg.id.name_str(),
                    algorithm
                );
                continue;
            }
        };

        if actual == expected {
            return Ok(());
        }
    }

    if has_weak_algorithm && !has_strong_algorithm {
        return Err(mg_types::MgError::Other(format!(
            "integrity check failed for '{}': only weak algorithms present (sha1/md5)",
            pkg.id.name_str()
        )));
    }

    Err(mg_types::MgError::Other(format!(
        "integrity mismatch for '{}': none of the SRI entries matched",
        pkg.id.name_str()
    )))
}

fn materialize_package_from_store(
    store: &ContentStore,
    source_root: &Path,
    target_root: &Path,
) -> MgResult<()> {
    let _ = store;
    hardlink_tree(source_root, target_root)
}

fn extracted_package_marker_path(root: &Path) -> PathBuf {
    root.join(".megagate-package-root.json")
}

fn expected_extracted_package_marker_from_bytes(
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
) -> MgResult<ExtractedPackageMarker> {
    let tarball_fingerprint = if pkg.integrity.is_empty() {
        compute_sha256_hex(tarball_bytes)
    } else {
        format!("integrity:{}", pkg.integrity)
    };
    Ok(ExtractedPackageMarker {
        name: pkg.id.name_str().to_string(),
        version: pkg.id.version().to_string(),
        integrity: (!pkg.integrity.is_empty()).then(|| pkg.integrity.clone()),
        tarball_sha256: tarball_fingerprint,
    })
}

fn read_extracted_package_marker(root: &Path) -> MgResult<Option<ExtractedPackageMarker>> {
    let path = extracted_package_marker_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to read extracted package marker '{}': {}",
            path.display(),
            err
        ))
    })?;
    let marker = serde_json::from_str(&contents).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to parse extracted package marker '{}': {}",
            path.display(),
            err
        ))
    })?;
    Ok(Some(marker))
}

fn write_extracted_package_marker(root: &Path, marker: &ExtractedPackageMarker) -> MgResult<()> {
    let path = extracted_package_marker_path(root);
    let payload = serde_json::to_vec_pretty(marker).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to serialize extracted package marker '{}': {}",
            path.display(),
            err
        ))
    })?;
    atomic_write(&path, &payload)?;
    Ok(())
}

fn ensure_extracted_package_root(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_path: &Path,
) -> MgResult<PathBuf> {
    let tarball = std::fs::read(tarball_path).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to read tarball '{}' for '{}': {}",
            tarball_path.display(),
            pkg.id.name_str(),
            err
        ))
    })?;
    ensure_extracted_package_root_from_bytes(layout, store, shared_cache, pkg, &tarball)
}

fn ensure_extracted_package_root_from_bytes(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
) -> MgResult<PathBuf> {
    let expected_marker = expected_extracted_package_marker_from_bytes(pkg, tarball_bytes)?;
    let canonical_root = shared_cache
        .map(|shared| shared.extracted_package_root(pkg))
        .unwrap_or_else(|| local_extracted_package_root(layout, pkg));
    let canonical_lock = extracted_package_root_lock(&canonical_root);
    let _canonical_guard = match canonical_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if canonical_root.join("package.json").exists()
        && read_extracted_package_marker(&canonical_root)?
            .as_ref()
            .map(|marker| marker == &expected_marker)
            .unwrap_or(false)
    {
        return Ok(canonical_root);
    }

    let extract_root = extracted_package_root(layout, pkg);
    if extract_root.exists() {
        std::fs::remove_dir_all(&extract_root).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to remove stale extract root '{}' for '{}': {}",
                extract_root.display(),
                pkg.id.name_str(),
                err
            ))
        })?;
    }
    std::fs::create_dir_all(&extract_root).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to create extract root '{}' for '{}': {}",
            extract_root.display(),
            pkg.id.name_str(),
            err
        ))
    })?;
    extract_tarball_from_reader(std::io::Cursor::new(tarball_bytes), &extract_root)
        .map_err(|e| mg_types::MgError::Other(e.to_string()))?;
    let package_root = locate_package_dir(&extract_root)?;

    if canonical_root.exists() {
        std::fs::remove_dir_all(&canonical_root).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to remove stale canonical root '{}' for '{}': {}",
                canonical_root.display(),
                pkg.id.name_str(),
                err
            ))
        })?;
    }
    materialize_package_from_store(store, &package_root, &canonical_root)?;
    write_extracted_package_marker(&canonical_root, &expected_marker)?;
    if extract_root.exists() {
        std::fs::remove_dir_all(&extract_root).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to clean extract root '{}' for '{}': {}",
                extract_root.display(),
                pkg.id.name_str(),
                err
            ))
        })?;
    }
    Ok(canonical_root)
}

/// Extract and materialize **all** packages in the resolved graph concurrently.
/// Returns a map: PackageId → canonical extraction root (PathBuf).
async fn parallel_extract_packages(
    graph: &ResolvedGraph,
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
) -> MgResult<std::collections::HashMap<PackageId, PathBuf>> {
    let extract_concurrency = std::env::var("MEGAGATE_WEB_EXTRACT_CONCURRENCY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            // Use number of CPUs, but cap at 32 for extraction
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(32)
        });

    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(extract_concurrency));
    let mut join_set: tokio::task::JoinSet<MgResult<(PackageId, PathBuf)>> =
        tokio::task::JoinSet::new();

    for pkg in &graph.packages {
        let pkg = pkg.clone();
        let layout = (*layout).clone();
        let store = (*store).clone();
        let shared_cache = shared_cache.map(|s| (*s).clone());
        let cache = (*cache).clone();
        let sem = sem.clone();

        join_set.spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|e| mg_types::MgError::Other(format!("extract semaphore closed: {e}")))?;

            let tarball_path = cache.tarball_path(&pkg.id);
            // spawn_blocking: extraction is CPU-bound (decompression) + sync I/O
            let id = pkg.id.clone();
            let root = tokio::task::spawn_blocking(move || {
                // Read tarball bytes once from cache (already in memory from prefetch or quick disk read)
                let tarball_bytes = std::fs::read(&tarball_path).map_err(|e| {
                    mg_types::MgError::Other(format!(
                        "failed to read tarball '{}' for '{}': {}",
                        tarball_path.display(),
                        pkg.id.name_str(),
                        e
                    ))
                })?;
                ensure_extracted_package_root_from_bytes(
                    &layout,
                    &store,
                    shared_cache.as_ref(),
                    &pkg,
                    &tarball_bytes,
                )
            })
            .await
            .map_err(|e| mg_types::MgError::Other(format!("extract task panicked: {e}")))??;

            Ok((id, root))
        });
    }

    let mut results = std::collections::HashMap::new();
    while let Some(joined) = join_set.join_next().await {
        let (id, root) =
            joined.map_err(|e| mg_types::MgError::Other(format!("extract join failed: {e}")))??;
        results.insert(id, root);
    }

    Ok(results)
}

fn hardlink_tree(source_root: &Path, target_root: &Path) -> MgResult<()> {
    std::fs::create_dir_all(target_root).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to create target '{}': {}",
            target_root.display(),
            err
        ))
    })?;

    for entry in WalkDir::new(source_root) {
        let entry = entry.map_err(|e| mg_types::MgError::Other(e.to_string()))?;
        let path = entry.path();
        if path == source_root {
            continue;
        }

        let relative = path
            .strip_prefix(source_root)
            .map_err(|e| mg_types::MgError::Other(e.to_string()))?;
        let target = target_root.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to create directory '{}' while cloning '{}': {}",
                    target.display(),
                    source_root.display(),
                    err
                ))
            })?;
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let executable = is_executable(path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to create parent '{}' for '{}': {}",
                    parent.display(),
                    target.display(),
                    err
                ))
            })?;
        }
        if target.exists() {
            std::fs::remove_file(&target).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to remove existing file '{}' before clone: {}",
                    target.display(),
                    err
                ))
            })?;
        }
        match std::fs::hard_link(path, &target) {
            Ok(()) => {}
            Err(_) => {
                std::fs::copy(path, &target).map_err(|err| {
                    mg_types::MgError::Other(format!(
                        "failed to materialize '{}' to '{}': {}",
                        path.display(),
                        target.display(),
                        err
                    ))
                })?;
            }
        }
        set_executable(&target, executable)?;
    }

    Ok(())
}

fn extracted_root_for(
    extracted_roots: &mut std::collections::HashMap<PackageId, PathBuf>,
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    pkg: &ResolvedPackage,
) -> MgResult<PathBuf> {
    if let Some(existing) = extracted_roots.get(&pkg.id) {
        return Ok(existing.clone());
    }

    let tarball_path = cache.tarball_path(&pkg.id);
    let root = ensure_extracted_package_root(layout, store, shared_cache, pkg, &tarball_path)?;
    extracted_roots.insert(pkg.id.clone(), root.clone());
    Ok(root)
}

fn select_root_packages(graph: &ResolvedGraph) -> Vec<&ResolvedPackage> {
    let mut selected: std::collections::HashMap<String, &ResolvedPackage> =
        std::collections::HashMap::new();

    for pkg in &graph.packages {
        selected
            .entry(pkg.id.name_str().to_string())
            .and_modify(|current| {
                let prefer_candidate = (pkg.direct && !current.direct)
                    || (pkg.direct == current.direct && pkg.id.version() > current.id.version());
                if prefer_candidate {
                    *current = pkg;
                }
            })
            .or_insert(pkg);
    }

    graph
        .packages
        .iter()
        .filter(|pkg| {
            selected
                .get(pkg.id.name_str())
                .map(|selected| selected.id == pkg.id)
                .unwrap_or(false)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn materialize_nested_dependencies(
    package_dir: &Path,
    pkg: &ResolvedPackage,
    package_map: &std::collections::HashMap<PackageId, &ResolvedPackage>,
    root_package_versions: &std::collections::HashMap<String, PackageId>,
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    extracted_roots: &mut std::collections::HashMap<PackageId, PathBuf>,
    visiting: &mut std::collections::HashSet<String>,
    depth: usize,
    packages_with_scripts: &mut Vec<std::path::PathBuf>,
) -> MgResult<()> {
    const MAX_DEPTH: usize = 50;
    if depth > MAX_DEPTH {
        return Err(mg_types::MgError::Other(format!(
            "dependency graph too deep (>{}) for '{}'",
            MAX_DEPTH,
            pkg.id.name_str()
        )));
    }
    if !visiting.insert(pkg.id.name_str().to_string()) {
        return Ok(());
    }

    let nested_node_modules = package_dir.join("node_modules");
    std::fs::create_dir_all(&nested_node_modules).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to create nested node_modules '{}' for '{}': {}",
            nested_node_modules.display(),
            pkg.id.name_str(),
            err
        ))
    })?;

    for dep_id in &pkg.deps {
        let Some(dep_pkg) = package_map.get(dep_id) else {
            continue;
        };
        let is_hoisted_match = root_package_versions
            .get(dep_id.name_str())
            .map(|root_id| root_id == dep_id)
            .unwrap_or(false);
        if is_hoisted_match {
            continue;
        }

        let nested_dir = nested_node_modules.join(dep_id.name().as_str());
        if !installed_package_matches(&nested_dir, dep_id) {
            if nested_dir.exists() {
                std::fs::remove_dir_all(&nested_dir).map_err(|err| {
                    mg_types::MgError::Other(format!(
                        "failed to remove stale nested dependency '{}' for '{}': {}",
                        nested_dir.display(),
                        dep_id,
                        err
                    ))
                })?;
            }

            let package_root =
                extracted_root_for(extracted_roots, layout, store, shared_cache, cache, dep_pkg)?;
            hardlink_tree(package_root.as_path(), &nested_dir)?;
            packages_with_scripts.push(nested_dir.clone());
        }

        materialize_nested_dependencies(
            &nested_dir,
            dep_pkg,
            package_map,
            root_package_versions,
            layout,
            store,
            shared_cache,
            cache,
            extracted_roots,
            visiting,
            depth + 1,
            packages_with_scripts,
        )?;
    }

    visiting.remove(pkg.id.name_str());
    Ok(())
}

fn locate_package_dir(extract_root: &Path) -> MgResult<std::path::PathBuf> {
    let package_dir = extract_root.join("package");
    if package_dir.is_dir() {
        return Ok(package_dir);
    }

    let first_dir = std::fs::read_dir(extract_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir());

    first_dir.ok_or_else(|| {
        mg_types::MgError::Other(format!(
            "extracted tarball missing package root in '{}'",
            extract_root.display()
        ))
    })
}

fn remove_fs_entry(path: &Path) -> MgResult<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(mg_types::MgError::Other(format!(
                "failed to inspect filesystem entry '{}': {}",
                path.display(),
                err
            )));
        }
    };

    if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to remove directory '{}': {}",
                path.display(),
                err
            ))
        })?;
    } else {
        std::fs::remove_file(path).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to remove file '{}': {}",
                path.display(),
                err
            ))
        })?;
    }

    Ok(())
}

fn reset_nested_node_modules(package_dir: &Path) -> MgResult<()> {
    let nested_node_modules = package_dir.join("node_modules");
    if nested_node_modules.exists() {
        remove_fs_entry(&nested_node_modules)?;
    }
    std::fs::create_dir_all(&nested_node_modules).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to recreate nested node_modules '{}': {}",
            nested_node_modules.display(),
            err
        ))
    })?;
    Ok(())
}

fn prune_root_install_dirs(
    node_modules: &Path,
    expected_root_packages: &std::collections::HashMap<String, PackageId>,
) -> MgResult<()> {
    if !node_modules.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(node_modules).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to read node_modules '{}': {}",
            node_modules.display(),
            err
        ))
    })? {
        let entry = entry.map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to iterate node_modules '{}': {}",
                node_modules.display(),
                err
            ))
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name == ".bin" || name == ".megagate" {
            continue;
        }

        if name.starts_with('@') && path.is_dir() {
            for scoped_entry in std::fs::read_dir(&path).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to read scoped directory '{}': {}",
                    path.display(),
                    err
                ))
            })? {
                let scoped_entry = scoped_entry.map_err(|err| {
                    mg_types::MgError::Other(format!(
                        "failed to iterate scoped directory '{}': {}",
                        path.display(),
                        err
                    ))
                })?;
                let scoped_name = scoped_entry.file_name().to_string_lossy().to_string();
                let package_name = format!("{}/{}", name, scoped_name);
                if !expected_root_packages.contains_key(&package_name) {
                    remove_fs_entry(&scoped_entry.path())?;
                }
            }

            let mut remaining = std::fs::read_dir(&path).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to re-read scoped directory '{}': {}",
                    path.display(),
                    err
                ))
            })?;
            if remaining.next().is_none() {
                remove_fs_entry(&path)?;
            }
            continue;
        }

        if !expected_root_packages.contains_key(&name) {
            remove_fs_entry(&path)?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PackageBinField {
    Single(String),
    Multiple(std::collections::HashMap<String, String>),
}

#[derive(Debug, Deserialize)]
struct InstalledPackageManifest {
    name: String,
    bin: Option<PackageBinField>,
}

fn rebuild_bin_links(node_modules: &Path, packages: &[&ResolvedPackage]) -> MgResult<()> {
    let bin_dir = node_modules.join(".bin");
    if bin_dir.exists() {
        std::fs::remove_dir_all(&bin_dir).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to remove stale bin dir '{}': {}",
                bin_dir.display(),
                err
            ))
        })?;
    }
    std::fs::create_dir_all(&bin_dir).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to create bin dir '{}': {}",
            bin_dir.display(),
            err
        ))
    })?;

    for pkg in packages {
        let package_dir = node_modules.join(pkg.id.name().as_str());
        for (bin_name, relative_target) in package_bin_entries(&package_dir)? {
            if relative_target
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                eprintln!(
                    "WARNING: Skipping bin '{}' from package '{}' - path contains '..'",
                    bin_name,
                    pkg.id.name_str()
                );
                continue;
            }

            let target = package_dir.join(&relative_target);
            if !target.exists() {
                continue;
            }

            let canonical_target = match target.canonicalize() {
                Ok(path) => path,
                Err(_) => {
                    eprintln!(
                        "WARNING: Skipping bin '{}' from package '{}' - cannot resolve path",
                        bin_name,
                        pkg.id.name_str()
                    );
                    continue;
                }
            };

            let canonical_package_dir = match package_dir.canonicalize() {
                Ok(path) => path,
                Err(_) => package_dir.clone(),
            };

            if !canonical_target.starts_with(&canonical_package_dir) {
                eprintln!(
                    "WARNING: Skipping bin '{}' from package '{}' - target escapes package directory",
                    bin_name,
                    pkg.id.name_str()
                );
                continue;
            }

            let link = bin_dir.join(bin_name);
            create_bin_link(&link, &target)?;
        }
    }

    Ok(())
}

fn package_bin_entries(package_dir: &Path) -> MgResult<Vec<(String, PathBuf)>> {
    let package_json = package_dir.join("package.json");
    if !package_json.exists() {
        return Ok(vec![]);
    }

    let manifest: InstalledPackageManifest =
        serde_json::from_str(&std::fs::read_to_string(&package_json)?).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to parse package manifest '{}': {}",
                package_json.display(),
                err
            ))
        })?;

    let entries = match manifest.bin {
        Some(PackageBinField::Single(path)) => vec![(
            manifest
                .name
                .rsplit('/')
                .next()
                .unwrap_or(manifest.name.as_str())
                .to_string(),
            PathBuf::from(path),
        )],
        Some(PackageBinField::Multiple(entries)) => entries
            .into_iter()
            .map(|(name, path)| (name, PathBuf::from(path)))
            .collect(),
        None => vec![],
    };

    Ok(entries)
}

#[cfg(unix)]
fn create_bin_link(link: &Path, target: &Path) -> MgResult<()> {
    use std::os::unix::fs::symlink;

    if link.exists() {
        std::fs::remove_file(link).map_err(|err| {
            mg_types::MgError::Other(format!(
                "failed to remove existing bin link '{}': {}",
                link.display(),
                err
            ))
        })?;
    }

    symlink(target, link).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to create bin link '{}' -> '{}': {}",
            link.display(),
            target.display(),
            err
        ))
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn create_bin_link(link: &Path, target: &Path) -> MgResult<()> {
    let command = format!("@echo off\r\n\"{}\" %*\r\n", target.display());
    std::fs::write(link.with_extension("cmd"), command).map_err(|err| {
        mg_types::MgError::Other(format!(
            "failed to create cmd shim for '{}' -> '{}': {}",
            link.display(),
            target.display(),
            err
        ))
    })?;
    Ok(())
}

fn is_executable(path: &Path) -> MgResult<bool> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode();
    Ok(mode & 0o111 != 0)
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> MgResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = if executable { 0o755 } else { 0o644 };
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> MgResult<bool> {
    Ok(false)
}

#[cfg(not(unix))]
fn set_executable(_: &Path, _: bool) -> MgResult<()> {
    Ok(())
}

fn write_web_lockfile_with_state(
    project_root: &Path,
    graph: &ResolvedGraph,
    state: &str,
) -> MgResult<()> {
    let lock_path = project_root.join("mg.lock");
    let mut lockfile = read_web_lockfile_checked(project_root)?
        .unwrap_or_else(|| Lockfile::new("web", "frontend"));

    let local_layout = Layout::new(project_cache_dir(project_root));
    let cache = PackageCache::new(local_layout.cache_dir())
        .map_err(|e| mg_types::MgError::Store(e.to_string()))
        .ok();

    lockfile.version = 1;
    lockfile.core = "web".to_string();
    lockfile.resolution = ResolutionMeta {
        state: state.to_string(),
        store: "megagate".to_string(),
        package_count: graph.packages.len(),
    };
    lockfile.packages = graph
        .packages
        .iter()
        .map(|pkg| {
            let integrity = if pkg.integrity.is_empty() {
                if let Some(ref cache) = cache {
                    if let Ok(Some(bytes)) = cache.get_tarball(&pkg.id) {
                        Some(compute_tarball_integrity(&bytes))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                Some(pkg.integrity.clone())
            };
            LockPackage {
                name: pkg.id.name_str().to_string(),
                version: pkg.id.version().to_string(),
                integrity,
                direct: pkg.direct,
                dev: pkg.dev,
                dependencies: pkg.deps.iter().map(ToString::to_string).collect(),
            }
        })
        .collect();

    mg_lockfile::LockfileSigner::sign(&mut lockfile)
        .map_err(|e| mg_types::MgError::Other(format!("lockfile signing failed: {e}")))?;

    let toml = serialization::to_toml(&lockfile)?;
    atomic_write(&lock_path, toml.as_bytes())?;

    mg_lockfile::write_lockfile_checksum(project_root, toml.as_bytes())
        .map_err(|e| mg_types::MgError::Other(format!("checksum failed: {e}")))?;

    Ok(())
}

fn installed_package_version(path: &Path) -> Option<Version> {
    let package_json = path.join("package.json");
    let contents = std::fs::read_to_string(package_json).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let version = value.get("version")?.as_str()?;
    Version::parse(version).ok()
}

fn installed_package_matches(path: &Path, package_id: &PackageId) -> bool {
    installed_package_version(path)
        .map(|version| version == *package_id.version())
        .unwrap_or(false)
}

pub fn read_web_lockfile(project_root: &Path) -> Option<Lockfile> {
    read_web_lockfile_checked(project_root).ok().flatten()
}

pub fn read_web_lockfile_checked(project_root: &Path) -> MgResult<Option<Lockfile>> {
    if strict_integrity_enforced()
        && std::env::var("MEGAGATE_WEB_SKIP_LOCKFILE_CHECKSUM").is_err()
        && mg_lockfile::lockfile_path(project_root).exists()
        && !mg_lockfile::lockfile_checksum_path(project_root).exists()
    {
        eprintln!(
            "WARNING: Lockfile checksum file (mg.lock.sha256) not found - cannot verify integrity"
        );
    }

    mg_lockfile::read_lockfile_checked(project_root)
        .map_err(|err| mg_types::MgError::Other(err.to_string()))
}

fn preferred_registry_version(metadata: &native::npm_registry::PackageMetadata) -> Option<String> {
    let stable_max = metadata
        .versions
        .keys()
        .filter_map(|v| Version::parse(v).ok())
        .filter(|v| v.pre.is_none())
        .max()
        .map(|v| v.to_string());

    if let Some(latest) = metadata.dist_tags.get("latest") {
        if let Ok(version) = Version::parse(latest) {
            if version.pre.is_none() {
                return Some(version.to_string());
            }
        }
    }

    stable_max
        .or_else(|| metadata.dist_tags.get("latest").cloned())
        .or_else(|| {
            metadata
                .versions
                .keys()
                .filter_map(|v| Version::parse(v).ok())
                .max()
                .map(|v| v.to_string())
        })
}

#[allow(clippy::too_many_arguments)]
fn materialize_strict_layout(
    project_root: &Path,
    node_modules: &Path,
    staged_node_modules: &Path,
    graph: &ResolvedGraph,
    package_map: &std::collections::HashMap<PackageId, &ResolvedPackage>,
    root_packages: &[&ResolvedPackage],
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    packages_with_scripts: &mut Vec<std::path::PathBuf>,
    extracted_roots: &std::collections::HashMap<PackageId, PathBuf>,
) -> MgResult<()> {
    let _ = project_root;
    let _ = staged_node_modules;
    let _ = store;
    let _ = shared_cache;
    let _ = cache;
    let virtual_store = node_modules.join(".megagate");
    if let Err(e) = std::fs::create_dir_all(&virtual_store) {
        return Err(mg_types::MgError::Other(format!(
            "failed to create virtual store: {}",
            e
        )));
    }

    // 1. Materialize all packages into virtual store - PARALLEL.
    // Keep project installs independent from shared-cache GC: hardlinks survive even
    // when the shared extracted root is later pruned.
    let vstore_dirs: Vec<_> = graph
        .packages
        .iter()
        .map(|pkg| {
            let pkg_id = &pkg.id;
            let vstore_pkg_name = format!(
                "{}@{}",
                pkg_id.name().as_str().replace('/', "+"),
                pkg_id.version()
            );
            let vstore_pkg_dir = virtual_store
                .join(&vstore_pkg_name)
                .join("node_modules")
                .join(pkg_id.name().as_str());
            (pkg_id.clone(), vstore_pkg_dir, pkg.clone())
        })
        .collect();

    // Parallel materialization: hardlink from canonical root to vstore.
    let materialize_results: Vec<_> = vstore_dirs
        .into_par_iter()
        .map(|(pkg_id, vstore_pkg_dir, pkg)| {
            if !installed_package_matches(&vstore_pkg_dir, &pkg_id) {
                remove_fs_entry(&vstore_pkg_dir)?;
                if let Some(parent) = vstore_pkg_dir.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        mg_types::MgError::Other(format!(
                            "failed to create vstore parent '{}': {}",
                            parent.display(),
                            err
                        ))
                    })?;
                }
                let package_root = extracted_roots
                    .get(&pkg_id)
                    .cloned()
                    .unwrap_or_else(|| local_extracted_package_root(layout, &pkg));
                hardlink_tree(&package_root, &vstore_pkg_dir)?;
            }
            Ok::<_, mg_types::MgError>(vstore_pkg_dir)
        })
        .collect();

    // Check for errors and collect packages_with_scripts
    for result in materialize_results {
        packages_with_scripts.push(result?);
    }

    // 2. Link dependencies within virtual store - PARALLEL
    let link_results: Vec<_> = graph
        .packages
        .par_iter()
        .map(|pkg| {
            let pkg_id = &pkg.id;
            let vstore_pkg_name = format!(
                "{}@{}",
                pkg_id.name().as_str().replace('/', "+"),
                pkg_id.version()
            );
            let vstore_node_modules = virtual_store.join(&vstore_pkg_name).join("node_modules");
            let pkg_local_node_modules = vstore_node_modules
                .join(pkg_id.name().as_str())
                .join("node_modules");
            std::fs::create_dir_all(&pkg_local_node_modules).map_err(|err| {
                mg_types::MgError::Other(format!(
                    "failed to create strict nested node_modules '{}' for '{}': {}",
                    pkg_local_node_modules.display(),
                    pkg_id.name_str(),
                    err
                ))
            })?;

            for dep_id in &pkg.deps {
                if let Some(_dep_pkg) = package_map.get(dep_id) {
                    let dep_vstore_name = format!(
                        "{}@{}",
                        dep_id.name().as_str().replace('/', "+"),
                        dep_id.version()
                    );
                    let dep_vstore_pkg_dir = virtual_store
                        .join(&dep_vstore_name)
                        .join("node_modules")
                        .join(dep_id.name().as_str());

                    let symlink_path = vstore_node_modules.join(dep_id.name().as_str());
                    if !symlink_path.exists() {
                        crate::layout::create_symlink(&dep_vstore_pkg_dir, &symlink_path)?;
                    }

                    let local_symlink_path = pkg_local_node_modules.join(dep_id.name().as_str());
                    if !local_symlink_path.exists() {
                        crate::layout::create_symlink(&dep_vstore_pkg_dir, &local_symlink_path)?;
                    }
                }
            }

            // NOTE: Removed redundant manifest re-read (was pass 2 lines 3416-3454)
            // The resolved graph already contains all deps; manifest read was duplicating work.

            Ok::<_, mg_types::MgError>(())
        })
        .collect();

    // Check for errors in parallel linking
    for result in link_results {
        result?;
    }

    // 3. Link root packages to root node_modules
    for pkg in root_packages {
        let root_link = node_modules.join(pkg.id.name().as_str());
        let vstore_pkg_name = format!(
            "{}@{}",
            pkg.id.name().as_str().replace('/', "+"),
            pkg.id.version()
        );
        let vstore_pkg_dir = virtual_store
            .join(&vstore_pkg_name)
            .join("node_modules")
            .join(pkg.id.name().as_str());

        if root_link.exists() {
            let _ = std::fs::remove_dir_all(&root_link);
        }
        crate::layout::create_symlink(&vstore_pkg_dir, &root_link)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::ErrorKind;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::sync::{Mutex, OnceLock};
    use tar::{Builder, Header};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn bind_test_listener() -> Option<TcpListener> {
        match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => Some(listener),
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!("skipping socket-backed test in sandbox: {err}");
                None
            }
            Err(err) => panic!("failed to bind socket-backed test listener: {err}"),
        }
    }

    #[test]
    fn test_web_adapter() {
        assert_eq!(WebAdapter::new().registry_url, "https://registry.npmjs.org");
    }
    #[test]
    fn test_package_json() {
        let p = PackageJson::new("t".into(), "1.0.0".into());
        assert_eq!(p.name, "t");
    }
    #[test]
    fn test_can_handle() {
        let dir = tempfile::tempdir().unwrap();
        PackageJson::new("t".into(), "1.0.0".into())
            .save(&dir.path().join("package.json"))
            .unwrap();
        assert!(WebAdapter::new().can_handle(dir.path()));
    }

    #[tokio::test]
    async fn test_add_writes_manifest_and_install_creates_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "private": true,
                "type": "module",
                "scripts": {
                    "dev": "mg web dev"
                }
            })
            .to_string(),
        )
        .unwrap();

        let adapter = WebAdapter::new();
        let name = PackageName::new("tailwindcss").unwrap();
        let range = VersionRange::parse("^3.4.0").unwrap();
        adapter
            .add(dir.path(), &name, Some(&range), AddOptions::default())
            .await
            .unwrap();

        let manifest = adapter.parse_manifest(dir.path()).await.unwrap();
        assert!(manifest.find_dep("tailwindcss").is_some());
        let package_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(package_json.contains("\"private\": true"));
        assert!(package_json.contains("\"type\": \"module\""));
        assert!(package_json.contains("\"dev\": \"mg web dev\""));

        let package_id = PackageId::new(name, Version::parse("3.4.0").unwrap());
        let integrity = seed_cached_tarball(dir.path(), &package_id);
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert!(dir
            .path()
            .join("node_modules")
            .join("tailwindcss")
            .join("package.json")
            .exists());
        assert!(dir
            .path()
            .join("node_modules")
            .join("tailwindcss")
            .join("index.css")
            .exists());

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "locked");
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].name, "tailwindcss");
        assert_eq!(parsed.packages[0].version, "3.4.0");
    }

    #[tokio::test]
    async fn test_install_materializes_node_modules_bin_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "vite": "^8.1.4"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("vite").unwrap(),
            Version::parse("8.1.4").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &package_id,
            &[
                (
                    "package/package.json",
                    br#"{"name":"vite","version":"8.1.4","bin":"bin/vite.js"}"#.as_slice(),
                ),
                (
                    "package/bin/vite.js",
                    b"#!/usr/bin/env node\nconsole.log('vite')\n",
                ),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id,
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                direct: true,
                dev: true,
            }],
        };

        let adapter = WebAdapter::new();
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        assert!(dir.path().join("node_modules/.bin").exists());
        assert!(dir.path().join("node_modules/.bin/vite").exists());
    }

    #[tokio::test]
    async fn test_resolve_populates_tarball_url_and_integrity_from_shared_metadata() {
        let shared = tempfile::tempdir().unwrap();

        seed_shared_metadata(
            shared.path(),
            "react",
            serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/react-18.2.0.tgz",
                            "integrity": "sha512-c2hhcmVk"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }),
        );

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let mut manifest = Manifest::new("demo", mg_types::ecosystem::Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );

        let graph = adapter.resolve(&manifest).await.unwrap();
        assert_eq!(graph.packages.len(), 1);
        assert_eq!(
            graph.packages[0].tarball_url,
            "https://registry.example.test/react-18.2.0.tgz"
        );
        assert_eq!(graph.packages[0].integrity, "sha512-c2hhcmVk");
    }

    #[tokio::test]
    async fn test_resolve_uses_shared_resolution_cache_when_registry_is_unavailable() {
        let shared = tempfile::tempdir().unwrap();
        let registry_url = "http://127.0.0.1:9";
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let mut manifest = Manifest::new("demo-a", mg_types::ecosystem::Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: PackageId::new(
                    PackageName::new("react").unwrap(),
                    Version::parse("18.2.0").unwrap(),
                ),
                integrity: "sha512-react".to_string(),
                tarball_url: "https://registry.example.test/react-18.2.0.tgz".to_string(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let key = manifest_resolution_cache_key(&manifest, registry_url);
        cache.write_resolution(&key, registry_url, &graph).unwrap();

        let adapter = WebAdapter::with_registry_and_shared_cache(
            registry_url.to_string(),
            shared.path().to_path_buf(),
        );
        let resolved = adapter.resolve(&manifest).await.unwrap();

        assert_eq!(resolved.packages.len(), 1);
        assert_eq!(resolved.packages[0].id.to_string(), "react@18.2.0");
        assert_eq!(resolved.packages[0].integrity, "sha512-react");
    }

    #[test]
    fn test_read_web_lockfile_checked_rejects_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution.state = "locked".into();
        let toml = serialization::to_toml(&lock).unwrap();
        std::fs::write(dir.path().join("mg.lock"), toml).unwrap();
        std::fs::write(dir.path().join("mg.lock.sha256"), "not-the-checksum").unwrap();

        let err = read_web_lockfile_checked(dir.path()).unwrap_err();

        assert!(
            err.to_string().contains("lockfile checksum mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_read_web_lockfile_checked_rejects_malformed_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mg.lock"), "not = [valid").unwrap();

        let err = read_web_lockfile_checked(dir.path()).unwrap_err();

        assert!(
            err.to_string().contains("failed to parse lockfile"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_manifest_resolution_cache_key_ignores_dep_order_and_app_name() {
        let registry_url = "https://registry.npmjs.org";
        let mut left = Manifest::new("demo-a", mg_types::ecosystem::Ecosystem::Web);
        left.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        left.add_dep(
            DependencySpec::new(
                PackageName::new("zod").unwrap(),
                VersionRange::parse("^3.22.4").unwrap(),
            ),
            true,
            false,
            false,
        );

        let mut right = Manifest::new("demo-b", mg_types::ecosystem::Ecosystem::Web);
        right.add_dep(
            DependencySpec::new(
                PackageName::new("zod").unwrap(),
                VersionRange::parse("^3.22.4").unwrap(),
            ),
            true,
            false,
            false,
        );
        right.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );

        assert_eq!(
            manifest_resolution_cache_key(&left, registry_url),
            manifest_resolution_cache_key(&right, registry_url)
        );
    }

    #[test]
    fn test_prune_shared_cache_to_quota_removes_prunable_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache").join("react");
        let resolution_dir = dir.path().join("resolutions");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::create_dir_all(&resolution_dir).unwrap();
        std::fs::write(cache_dir.join("18.2.0.tgz"), vec![b'a'; 1024]).unwrap();
        std::fs::write(resolution_dir.join("graph.json"), vec![b'b'; 1024]).unwrap();

        prune_shared_cache_to_quota(dir.path(), 512).unwrap();

        let remaining = directory_size(dir.path());
        assert!(
            remaining <= 512,
            "expected quota prune to reduce cache to <= 512 bytes, got {remaining}"
        );
    }

    #[test]
    fn test_prune_shared_cache_to_quota_does_not_delete_unmarked_package_json_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("packages").join("manual").join("nested");
        let cache_dir = dir.path().join("cache").join("react");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(nested.join("package.json"), "{}").unwrap();
        std::fs::write(cache_dir.join("old.tgz"), vec![b'a'; 1024]).unwrap();

        prune_shared_cache_to_quota(dir.path(), 1).unwrap();

        assert!(
            nested.join("package.json").exists(),
            "quota pruning should only delete MegaGate-marked package cache roots"
        );
    }

    #[tokio::test]
    async fn test_alias_dependency_uses_target_metadata_and_range() {
        let shared = tempfile::tempdir().unwrap();

        seed_shared_metadata(
            shared.path(),
            "demo-parent",
            serde_json::json!({
                "name": "demo-parent",
                "description": null,
                "versions": {
                    "1.0.0": {
                        "version": "1.0.0",
                        "dependencies": {
                            "strip-ansi-cjs": "npm:strip-ansi@^6.0.1"
                        },
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/demo-parent-1.0.0.tgz",
                            "integrity": "sha512-parent"
                        }
                    }
                },
                "dist-tags": { "latest": "1.0.0" }
            }),
        );
        seed_shared_metadata(
            shared.path(),
            "strip-ansi",
            serde_json::json!({
                "name": "strip-ansi",
                "description": null,
                "versions": {
                    "6.0.1": {
                        "version": "6.0.1",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/strip-ansi-6.0.1.tgz",
                            "integrity": "sha512-strip-ansi"
                        }
                    }
                },
                "dist-tags": { "latest": "6.0.1" }
            }),
        );

        let provider = NpmDependencyProvider::new(
            "http://127.0.0.1:9",
            Some(SharedWebCache {
                root: shared.path().to_path_buf(),
            }),
        );
        let parent_id = PackageId::new(
            PackageName::new("demo-parent").unwrap(),
            Version::parse("1.0.0").unwrap(),
        );

        let deps = provider.get_dependencies(&parent_id).await.unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].package.as_str(), "strip-ansi-cjs");
        assert_eq!(deps[0].spec, "^6.0.1");

        let versions = provider.get_versions(&deps[0].package).await.unwrap();
        assert!(versions
            .iter()
            .any(|version| version.to_string() == "6.0.1"));
    }

    #[tokio::test]
    async fn test_load_metadata_persists_etag_after_initial_fetch() {
        let shared = tempfile::tempdir().unwrap();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_server = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits_for_server.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"name":"react","description":null,"versions":{"18.2.0":{"version":"18.2.0","dependencies":null,"optionalDependencies":null,"os":null,"cpu":null,"dist":{"tarball":"http://example.test/react.tgz","integrity":"sha512-react"}}},"dist-tags":{"latest":"18.2.0"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"react-v1\"\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let registry = native::npm_registry::NpmRegistry::new(&format!("http://{addr}"));
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };

        let metadata = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap();
        assert_eq!(metadata.name, "react");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let cached = std::fs::read_to_string(
            shared
                .path()
                .join("metadata")
                .join("react")
                .join("metadata.json"),
        )
        .unwrap();
        assert!(cached.contains("\"etag\":\"\\\"react-v1\\\"\""));
    }

    #[tokio::test]
    async fn test_stale_metadata_failure_sets_retry_cooldown() {
        let _env_guard = env_test_lock().lock().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_server = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits_for_server.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad",
                    )
                    .await;
            }
        });

        let registry = native::npm_registry::NpmRegistry::new(&format!("http://{addr}"));
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let metadata: native::npm_registry::PackageMetadata =
            serde_json::from_value(serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://example.test/react.tgz",
                            "integrity": "sha512-react"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }))
            .unwrap();

        cache
            .write_metadata_record(
                "react",
                &metadata,
                Some("\"react-v1\"".to_string()),
                current_unix_secs().saturating_sub(metadata_ttl_secs() + 1),
                None,
            )
            .unwrap();

        let previous_max_stale = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");
        std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "604800");

        let first = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap();
        let hits_after_first = hits.load(Ordering::SeqCst);
        let second = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap();
        restore_env_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous_max_stale);

        assert_eq!(first.name, "react");
        assert_eq!(second.name, "react");
        assert!(hits_after_first >= 1);
        assert_eq!(hits.load(Ordering::SeqCst), hits_after_first);

        let cached = cache.read_metadata("react").unwrap().unwrap();
        assert!(cached.stale_retry_after.is_some());
        assert!(metadata_record_retry_deferred(&cached));
    }

    #[tokio::test]
    async fn test_stale_metadata_too_old_is_not_reused_when_network_fails() {
        let _env_guard = env_test_lock().lock().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad",
                    )
                    .await;
            }
        });

        let registry = native::npm_registry::NpmRegistry::new(&format!("http://{addr}"));
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let metadata: native::npm_registry::PackageMetadata =
            serde_json::from_value(serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://example.test/react.tgz",
                            "integrity": "sha512-react"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }))
            .unwrap();

        let previous = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");
        std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "60");
        cache
            .write_metadata_record(
                "react",
                &metadata,
                Some("\"react-v1\"".to_string()),
                1,
                None,
            )
            .unwrap();

        let err = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap_err();
        restore_env_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous);

        assert!(err
            .to_string()
            .contains("cached metadata is too old to reuse"));
    }

    #[tokio::test]
    async fn test_retry_deferred_does_not_bypass_max_stale_limit() {
        let _env_guard = env_test_lock().lock().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let registry = native::npm_registry::NpmRegistry::new("http://127.0.0.1:9");
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let metadata: native::npm_registry::PackageMetadata =
            serde_json::from_value(serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://example.test/react.tgz",
                            "integrity": "sha512-react"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }))
            .unwrap();

        let previous = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");
        std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "60");
        cache
            .write_metadata_record(
                "react",
                &metadata,
                Some("\"react-v1\"".to_string()),
                1,
                Some(current_unix_secs().saturating_add(60)),
            )
            .unwrap();

        let err = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap_err();
        restore_env_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous);

        assert!(err
            .to_string()
            .contains("cached metadata is too old to reuse"));
    }

    #[tokio::test]
    async fn test_add_uses_shared_metadata_cache_when_registry_is_unavailable() {
        let shared = tempfile::tempdir().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0"
            })
            .to_string(),
        )
        .unwrap();

        seed_shared_metadata(
            shared.path(),
            "react",
            serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://127.0.0.1:9/react.tgz",
                            "integrity": null
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }),
        );

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let package_id = adapter
            .add(
                dir.path(),
                &PackageName::new("react").unwrap(),
                None,
                AddOptions::default(),
            )
            .await
            .unwrap();

        assert_eq!(package_id.version().to_string(), "18.2.0");
        let package_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(package_json.contains("\"react\": \"^18.2.0\""));
    }

    #[tokio::test]
    async fn test_parse_manifest_ignores_workspace_protocol_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "frontend",
                "version": "0.1.0",
                "dependencies": {
                    "@core/shared": "workspace:*",
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let adapter = WebAdapter::new();
        let manifest = adapter.parse_manifest(dir.path()).await.unwrap();
        assert!(manifest.find_dep("react").is_some());
        assert!(manifest.find_dep("@core/shared").is_none());
    }

    #[tokio::test]
    async fn test_list_prefers_lockfile_state() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_dir = dir.path().join("node_modules").join("tailwindcss");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            package_dir.join("package.json"),
            "{\"name\":\"tailwindcss\",\"version\":\"4.3.2\"}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("mg.lock"),
            serialization::to_toml(&Lockfile {
                version: 1,
                core: "web".into(),
                mode: "frontend".into(),
                frameworks: vec![],
                resolution: ResolutionMeta {
                    state: "locked".into(),
                    store: "megagate".into(),
                    package_count: 1,
                },
                workspaces: vec![],
                packages: vec![LockPackage {
                    name: "tailwindcss".into(),
                    version: "4.3.2".into(),
                    integrity: Some("sha256-test".into()),
                    direct: true,
                    dev: false,
                    dependencies: vec![],
                }],
                sig: None,
            })
            .unwrap(),
        )
        .unwrap();

        let adapter = WebAdapter::new();
        let installed = adapter.list(dir.path()).await.unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id.name_str(), "tailwindcss");
        assert_eq!(installed[0].id.version().to_string(), "4.3.2");
        assert_eq!(installed[0].integrity.as_deref(), Some("sha256-test"));
    }

    #[tokio::test]
    async fn test_install_multiple_packages_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0",
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let tailwind = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );

        let react_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export const version = '18.2.0';"),
            ],
        );
        let tailwind_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &tailwind,
            &[
                (
                    "package/package.json",
                    br#"{"name":"tailwindcss","version":"4.3.2"}"#.as_slice(),
                ),
                ("package/index.css", b"@import 'tailwindcss';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: react.clone(),
                    integrity: react_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: tailwind.clone(),
                    integrity: tailwind_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: true,
                    dev: false,
                },
            ],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added.len(), 2);
        assert!(summary.bytes_from_cache > 0);
        assert!(dir.path().join("node_modules/react/index.js").exists());
        assert!(dir
            .path()
            .join("node_modules/tailwindcss/index.css")
            .exists());

        let installed = adapter.list(dir.path()).await.unwrap();
        assert_eq!(installed.len(), 2);
    }

    #[tokio::test]
    async fn test_install_finalizes_lock_and_cleans_staging_tmp() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0",
                    "@types/react": "^19.2.17"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let react_types = PackageId::new(
            PackageName::new("@types/react").unwrap(),
            Version::parse("19.2.17").unwrap(),
        );

        let react_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );
        let react_types_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react_types,
            &[
                (
                    "package/package.json",
                    br#"{"name":"@types/react","version":"19.2.17"}"#.as_slice(),
                ),
                ("package/index.d.ts", b"export = React;"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: react.clone(),
                    integrity: react_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: react_types.clone(),
                    integrity: react_types_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: true,
                    dev: true,
                },
            ],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added.len(), 2);
        assert!(dir.path().join("node_modules/react/index.js").exists());
        assert!(dir
            .path()
            .join("node_modules/@types/react/index.d.ts")
            .exists());

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "locked");
        assert_eq!(parsed.resolution.package_count, 2);

        let tmp_dir = dir
            .path()
            .join(".megagate")
            .join("cache")
            .join("web")
            .join("tmp");
        let lingering_entries = std::fs::read_dir(&tmp_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert!(
            lingering_entries.is_empty(),
            "expected staging tmp to be cleaned, found {:?}",
            lingering_entries
                .iter()
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_install_uses_cache_when_registry_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );
        let integrity = seed_cached_tarball(dir.path(), &package_id);

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity,
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
        let summary = adapter
            .install(
                &graph,
                dir.path(),
                InstallOptions {
                    legacy_flat: true,
                    ..InstallOptions::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert!(summary.bytes_from_cache > 0);
    }

    #[tokio::test]
    async fn test_install_uses_shared_tarball_cache_for_new_project() {
        let shared = tempfile::tempdir().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_shared_tarball_with_files(
            shared.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity,
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![react.clone()]);
        assert!(summary.bytes_from_cache > 0);
        assert!(dir.path().join("node_modules/react/index.js").exists());
        assert!(shared
            .path()
            .join("cache")
            .join("react")
            .join("18.2.0.tgz")
            .exists());
    }

    #[tokio::test]
    async fn test_install_recovers_from_corrupted_local_cache_using_shared_cache() {
        let shared = tempfile::tempdir().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let files = vec![
            (
                "package/package.json",
                br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
            ),
            ("package/index.js", b"export default 'react';".as_slice()),
        ];
        let good_tarball = build_tarball_bytes(&files);
        seed_shared_tarball_with_files(shared.path(), &react, &files);
        let local_layout = Layout::new(dir.path().join(".megagate").join("cache").join("web"));
        std::fs::create_dir_all(local_layout.root()).unwrap();
        let local_cache = PackageCache::new(local_layout.cache_dir()).unwrap();
        local_cache.cache_tarball(&react, b"corrupted").unwrap();

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity: sri_sha512(&good_tarball),
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![react.clone()]);
        assert!(dir.path().join("node_modules/react/index.js").exists());

        let repaired = local_cache.get_tarball(&react).unwrap().unwrap();
        assert_eq!(repaired, good_tarball);
    }

    #[tokio::test]
    async fn test_install_fails_when_registry_is_unavailable_and_cache_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id,
                integrity: String::new(),
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::build("http://127.0.0.1:9".into(), None);
        let err = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("download failed"));
    }

    #[tokio::test]
    async fn test_install_failure_does_not_materialize_partial_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0",
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let missing = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );

        let react_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: react,
                    integrity: react_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: missing,
                    integrity: String::new(),
                    tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                    deps: vec![],
                    direct: true,
                    dev: false,
                },
            ],
        };

        let adapter = WebAdapter::build("http://127.0.0.1:9".into(), None);
        let _err = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap_err();
        assert!(!dir.path().join("node_modules/react").exists());

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "installing");
        assert_eq!(parsed.packages.len(), 2);
    }

    #[tokio::test]
    async fn test_install_skips_when_matching_package_is_already_materialized() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "zod": "^4.4.3"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("zod").unwrap(),
            Version::parse("4.4.3").unwrap(),
        );
        let pkg_dir = dir.path().join("node_modules").join("zod");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("package.json"),
            br#"{"name":"zod","version":"4.4.3"}"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.join("marker.txt"), "keep-me").unwrap();

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity: String::new(),
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
        let summary = adapter
            .install(
                &graph,
                dir.path(),
                InstallOptions {
                    legacy_flat: true,
                    ..InstallOptions::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert_eq!(
            std::fs::read_to_string(pkg_dir.join("marker.txt")).unwrap(),
            "keep-me"
        );

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "locked");
        assert_eq!(parsed.packages[0].version, "4.4.3");
    }

    #[tokio::test]
    async fn test_install_materializes_scoped_package() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "devDependencies": {
                    "@types/node": "26.1.1"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("@types/node").unwrap(),
            Version::parse("26.1.1").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &package_id,
            &[
                (
                    "package/package.json",
                    br#"{"name":"@types/node","version":"26.1.1"}"#.as_slice(),
                ),
                ("package/index.d.ts", b"export {};"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                direct: true,
                dev: true,
            }],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert!(dir
            .path()
            .join("node_modules")
            .join("@types")
            .join("node")
            .join("package.json")
            .exists());
        assert!(dir
            .path()
            .join("node_modules")
            .join("@types")
            .join("node")
            .join("index.d.ts")
            .exists());
    }

    #[tokio::test]
    async fn test_install_materializes_nested_conflicting_dependency_versions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "@nuxt/kit": "1.0.0",
                    "legacy-tool": "1.0.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let nuxt_kit = PackageId::new(
            PackageName::new("@nuxt/kit").unwrap(),
            Version::parse("1.0.0").unwrap(),
        );
        let legacy_tool = PackageId::new(
            PackageName::new("legacy-tool").unwrap(),
            Version::parse("1.0.0").unwrap(),
        );
        let semver7 = PackageId::new(
            PackageName::new("semver").unwrap(),
            Version::parse("7.8.5").unwrap(),
        );
        let semver6 = PackageId::new(
            PackageName::new("semver").unwrap(),
            Version::parse("6.3.1").unwrap(),
        );

        let nuxt_kit_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &nuxt_kit,
            &[(
                "package/package.json",
                br#"{"name":"@nuxt/kit","version":"1.0.0"}"#.as_slice(),
            )],
        );
        let legacy_tool_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &legacy_tool,
            &[(
                "package/package.json",
                br#"{"name":"legacy-tool","version":"1.0.0"}"#.as_slice(),
            )],
        );
        let semver7_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &semver7,
            &[
                (
                    "package/package.json",
                    br#"{"name":"semver","version":"7.8.5","exports":{"./functions/satisfies.js":"./functions/satisfies.js"}}"#
                        .as_slice(),
                ),
                ("package/functions/satisfies.js", b"export default true;\n"),
            ],
        );
        let semver6_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &semver6,
            &[(
                "package/package.json",
                br#"{"name":"semver","version":"6.3.1"}"#.as_slice(),
            )],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: nuxt_kit.clone(),
                    integrity: nuxt_kit_integrity,
                    tarball_url: String::new(),
                    deps: vec![semver7.clone()],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: legacy_tool.clone(),
                    integrity: legacy_tool_integrity,
                    tarball_url: String::new(),
                    deps: vec![semver6.clone()],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: semver7.clone(),
                    integrity: semver7_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: false,
                    dev: false,
                },
                ResolvedPackage {
                    id: semver6.clone(),
                    integrity: semver6_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    direct: false,
                    dev: false,
                },
            ],
        };

        let adapter = WebAdapter::new();
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        let hoisted_semver = dir.path().join("node_modules").join("semver");
        let nested_nuxt_semver = dir
            .path()
            .join("node_modules")
            .join("@nuxt")
            .join("kit")
            .join("node_modules")
            .join("semver");
        let nuxt_semver_dir = if nested_nuxt_semver.exists() {
            nested_nuxt_semver
        } else {
            hoisted_semver
        };
        assert!(nuxt_semver_dir
            .join("functions")
            .join("satisfies.js")
            .exists());
        assert_eq!(
            installed_package_version(&nuxt_semver_dir)
                .unwrap()
                .to_string(),
            "7.8.5"
        );
        assert_eq!(
            installed_package_version(
                &dir.path()
                    .join("node_modules")
                    .join("legacy-tool")
                    .join("node_modules")
                    .join("semver"),
            )
            .unwrap()
            .to_string(),
            "6.3.1"
        );
    }

    #[tokio::test]
    async fn test_install_retries_flaky_tarball_download() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let tarball = build_tarball_bytes(&[
            (
                "package/package.json",
                br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
            ),
            ("package/index.js", b"export default 'react';"),
        ]);
        let integrity = sri_sha512(&tarball);
        let tarball_for_server = tarball.clone();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut attempts = 0usize;
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                attempts += 1;
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                if attempts == 1 {
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\n\r\nnope",
                        )
                        .await;
                } else {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        tarball_for_server.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(&tarball_for_server).await;
                }
            }
        });

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity,
                tarball_url: format!("http://{addr}/react-18.2.0.tgz"),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![react]);
        assert!(dir.path().join("node_modules/react/index.js").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_install_materialization_uses_hardlinks_from_cached_extract_root() {
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        let cache_key = extracted_package_cache_key(&graph.packages[0]);
        let cached_file = shared
            .path()
            .join("packages")
            .join("react")
            .join(cache_key)
            .join("package")
            .join("index.js");
        let installed_file = dir
            .path()
            .join("node_modules")
            .join("react")
            .join("index.js");
        let cached_meta = std::fs::metadata(&cached_file)
            .unwrap_or_else(|_| panic!("cached file not found at: {}", cached_file.display()));
        let installed_meta = std::fs::metadata(&installed_file).unwrap_or_else(|_| {
            panic!("installed file not found at: {}", installed_file.display())
        });
        assert_eq!(cached_meta.ino(), installed_meta.ino());

        std::fs::remove_dir_all(shared.path().join("packages")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&installed_file).unwrap(),
            "export default 'react';"
        );
    }

    #[tokio::test]
    async fn test_install_rebuilds_shared_extracted_root_when_marker_mismatches() {
        let shared = tempfile::tempdir().unwrap();

        let first = tempfile::tempdir().unwrap();
        std::fs::write(
            first.path().join("package.json"),
            serde_json::json!({
                "name": "demo-a",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            first.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';\n"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, first.path(), InstallOptions::default())
            .await
            .unwrap();

        let shared_root = shared_extracted_package_root(shared.path(), &graph.packages[0]);
        std::fs::write(shared_root.join("index.js"), "tampered\n").unwrap();
        write_extracted_package_marker(
            &shared_root,
            &ExtractedPackageMarker {
                name: "react".into(),
                version: "18.2.0".into(),
                integrity: Some(integrity),
                tarball_sha256: "bad-digest".into(),
            },
        )
        .unwrap();

        let second = tempfile::tempdir().unwrap();
        std::fs::write(
            second.path().join("package.json"),
            serde_json::json!({
                "name": "demo-b",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();
        seed_cached_tarball_with_files(
            second.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';\n"),
            ],
        );

        adapter
            .install(&graph, second.path(), InstallOptions::default())
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(shared_root.join("index.js")).unwrap(),
            "export default 'react';\n"
        );
        assert_eq!(
            std::fs::read_to_string(second.path().join("node_modules/react/index.js")).unwrap(),
            "export default 'react';\n"
        );
        let marker = read_extracted_package_marker(&shared_root)
            .unwrap()
            .unwrap();
        assert_ne!(marker.tarball_sha256, "bad-digest");
    }

    fn seed_cached_tarball(root: &Path, pkg: &PackageId) -> String {
        let package_json = format!(
            "{{\"name\":\"{}\",\"version\":\"{}\"}}",
            pkg.name_str(),
            pkg.version()
        );
        seed_cached_tarball_with_files(
            root,
            pkg,
            &[
                ("package/package.json", package_json.as_bytes()),
                ("package/index.css", b"@import 'tailwindcss';"),
            ],
        )
    }

    fn seed_cached_tarball_with_files(
        root: &Path,
        pkg: &PackageId,
        files: &[(&str, &[u8])],
    ) -> String {
        let layout = Layout::new(root.join(".megagate").join("cache").join("web"));
        std::fs::create_dir_all(layout.root()).unwrap();
        let cache = PackageCache::new(layout.cache_dir()).unwrap();
        let tarball_path = cache.tarball_path(pkg);
        if let Some(parent) = tarball_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let tarball = build_tarball_bytes(files);
        std::fs::write(&tarball_path, &tarball).unwrap();
        sri_sha512(&tarball)
    }

    fn seed_shared_tarball_with_files(
        root: &Path,
        pkg: &PackageId,
        files: &[(&str, &[u8])],
    ) -> String {
        let layout = Layout::new(root.to_path_buf());
        std::fs::create_dir_all(layout.root()).unwrap();
        let cache = PackageCache::new(layout.cache_dir()).unwrap();
        let tarball_path = cache.tarball_path(pkg);
        if let Some(parent) = tarball_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let tarball = build_tarball_bytes(files);
        std::fs::write(&tarball_path, &tarball).unwrap();
        sri_sha512(&tarball)
    }

    fn seed_shared_metadata(root: &Path, package: &str, payload: serde_json::Value) {
        let path = root.join("metadata").join(package).join("metadata.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec(&payload).unwrap()).unwrap();
    }

    fn build_tarball_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let file = temp.reopen().unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (path, data) in files {
            write_tar_entry(&mut builder, path, data);
        }
        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
        std::fs::read(temp.path()).unwrap()
    }

    fn sri_sha512(data: &[u8]) -> String {
        let mut hasher = Sha512::new();
        hasher.update(data);
        format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
        )
    }

    fn restore_env_var(key: &str, previous: Option<std::ffi::OsString>) {
        if let Some(value) = previous {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    fn env_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_tar_entry(builder: &mut Builder<GzEncoder<std::fs::File>>, path: &str, data: &[u8]) {
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, data).unwrap();
    }

    #[test]
    fn test_preferred_saved_range_preserves_strategy() {
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("^4.0.0").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "^5.1.2"
        );
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("~4.0.0").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "~5.1.2"
        );
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("*").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "^5.1.2"
        );
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("4.0.0").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "5.1.2"
        );
    }

    #[test]
    fn test_preferred_registry_version_prefers_stable_over_prerelease() {
        let metadata = native::npm_registry::PackageMetadata {
            name: "demo".into(),
            description: None,
            versions: std::collections::HashMap::from([
                (
                    "4.4.3".into(),
                    native::npm_registry::VersionInfo {
                        version: "4.4.3".into(),
                        dependencies: None,
                        optional_dependencies: None,
                        os: None,
                        cpu: None,
                        dist: None,
                    },
                ),
                (
                    "4.5.0-canary.20260504T180558".into(),
                    native::npm_registry::VersionInfo {
                        version: "4.5.0-canary.20260504T180558".into(),
                        dependencies: None,
                        optional_dependencies: None,
                        os: None,
                        cpu: None,
                        dist: None,
                    },
                ),
            ]),
            dist_tags: std::collections::HashMap::from([(
                "latest".into(),
                "4.5.0-canary.20260504T180558".into(),
            )]),
        };

        assert_eq!(
            preferred_registry_version(&metadata).as_deref(),
            Some("4.4.3")
        );
    }

    #[test]
    fn test_installed_package_version_reads_real_version() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir.path().join("node_modules").join("zod");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("package.json"),
            br#"{"name":"zod","version":"4.4.3"}"#,
        )
        .unwrap();

        assert_eq!(
            installed_package_version(&pkg_dir).unwrap().to_string(),
            "4.4.3"
        );
    }

    #[test]
    fn test_known_optional_native_binary_supported_only_matches_current_target() {
        let supported = PackageName::new(format!(
            "@esbuild/{}-{}",
            NpmDependencyProvider::current_npm_os(),
            NpmDependencyProvider::current_npm_cpu()
        ))
        .unwrap();
        let unsupported = PackageName::new("@esbuild/linux-s390x").unwrap();
        let unknown = PackageName::new("optional-but-not-native").unwrap();

        assert_eq!(
            NpmDependencyProvider::known_optional_native_binary_supported(&supported),
            Some(true)
        );
        assert_eq!(
            NpmDependencyProvider::known_optional_native_binary_supported(&unsupported),
            Some(false)
        );
        assert_eq!(
            NpmDependencyProvider::known_optional_native_binary_supported(&unknown),
            None
        );
    }

    #[test]
    fn test_installed_package_matches_version() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir.path().join("node_modules").join("zod");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("package.json"),
            br#"{"name":"zod","version":"4.4.3"}"#,
        )
        .unwrap();
        let package_id = PackageId::new(
            PackageName::new("zod").unwrap(),
            Version::parse("4.4.3").unwrap(),
        );
        assert!(installed_package_matches(&pkg_dir, &package_id));
    }
}
