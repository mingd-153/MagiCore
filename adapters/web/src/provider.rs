//! `provider.rs` — `NpmDependencyProvider` implementation for `mgc-resolver`.

use async_trait::async_trait;
use dashmap::DashMap;
use futures_util::stream::{self, StreamExt};
use mgc_resolver::{DependencyError, DependencyProvider, RegistryCache, ResolvedDep};
use mgc_types::{PackageId, PackageName, Version, VersionRange};
use std::sync::Arc;

use crate::cache::{
    load_metadata_with_fallback, metadata_concurrency_limit, MetadataCache, SharedWebCache,
};
use crate::native;

pub struct NpmDependencyProvider {
    pub registry: native::npm_registry::NpmRegistry,
    pub metadata_cache: MetadataCache,
    pub metadata_locks: DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    pub registry_cache: RegistryCache,
    pub shared_cache: Option<SharedWebCache>,
    pub alias_targets: DashMap<String, PackageName>,
    pub optional_enqueue_cache: DashMap<String, bool>,
}

impl NpmDependencyProvider {
    pub fn new(url: &str, token: Option<String>, shared_cache: Option<SharedWebCache>) -> Self {
        Self::new_with_chain(url, token, Vec::new(), shared_cache)
    }

    pub fn new_with_chain(
        url: &str,
        token: Option<String>,
        fallbacks: Vec<(String, Option<String>)>,
        shared_cache: Option<SharedWebCache>,
    ) -> Self {
        Self {
            registry: native::npm_registry::NpmRegistry::new_with_chain(url, token, fallbacks),
            metadata_cache: MetadataCache::new(),
            metadata_locks: DashMap::new(),
            registry_cache: RegistryCache::new(),
            shared_cache,
            alias_targets: DashMap::new(),
            optional_enqueue_cache: DashMap::new(),
        }
    }

    pub async fn metadata(
        &self,
        package: &PackageName,
    ) -> Result<Arc<native::npm_registry::PackageMetadata>, DependencyError> {
        let source_package = self.source_package_name(package);
        let key = source_package.as_str().to_string();
        if let Some(cached) = self.metadata_cache.get(&key) {
            return Ok(cached);
        }
        let lock = self
            .metadata_locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        if let Some(cached) = self.metadata_cache.get(&key) {
            return Ok(cached);
        }
        let meta = load_metadata_with_fallback(
            &source_package,
            &self.registry,
            self.shared_cache.as_ref(),
        )
        .await?;
        self.metadata_cache.insert(key, Arc::clone(&meta));
        Ok(meta)
    }

    pub fn source_package_name(&self, package: &PackageName) -> PackageName {
        self.alias_targets
            .get(package.as_str())
            .map(|entry| entry.clone())
            .unwrap_or_else(|| package.clone())
    }

    pub fn record_alias_target(&self, alias: &PackageName, target: &PackageName) {
        self.alias_targets
            .insert(alias.as_str().to_string(), target.clone());
    }

    pub fn cached_versions_for(&self, package: &PackageName) -> Option<Vec<Version>> {
        self.registry_cache
            .get_versions(package.as_str())
            .or_else(|| {
                let source = self.source_package_name(package);
                if source == *package {
                    None
                } else {
                    self.registry_cache.get_versions(source.as_str())
                }
            })
    }

    pub fn insert_versions_for(&self, package: &PackageName, versions: Vec<Version>) {
        self.registry_cache
            .insert_versions(package.as_str().to_string(), versions.clone());
        let source = self.source_package_name(package);
        if source != *package {
            self.registry_cache
                .insert_versions(source.as_str().to_string(), versions);
        }
    }

    pub fn parse_alias_spec(spec: &str) -> Option<(String, String)> {
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

    pub fn collect_resolved_deps(
        &self,
        deps: Option<&std::collections::HashMap<String, String>>,
        optional: bool,
        peer: bool,
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
                        peer,
                    })
                } else {
                    Some(ResolvedDep {
                        package: alias,
                        spec: spec.clone(),
                        optional,
                        peer,
                    })
                }
            })
            .collect()
    }

    pub async fn prefetch_resolution_metadata(
        &self,
        names: &[PackageName],
    ) -> Result<
        std::collections::HashMap<String, Arc<native::npm_registry::PackageMetadata>>,
        DependencyError,
    > {
        let mut results = std::collections::HashMap::new();
        let mut alias_to_source = Vec::new();
        let mut source_names = Vec::new();
        let mut seen_sources = std::collections::HashSet::new();
        let mut futures = Vec::new();

        for alias_name in names {
            let alias_name = alias_name.clone();
            let source_name = self.source_package_name(&alias_name);
            alias_to_source.push((alias_name.as_str().to_string(), source_name.clone()));
            if let Some(metadata) = self.metadata_cache.get(source_name.as_str()) {
                results.insert(alias_name.as_str().to_string(), metadata.clone());
                continue;
            }
            if seen_sources.insert(source_name.as_str().to_string()) {
                source_names.push(source_name);
            }
        }

        for source_name in source_names {
            futures.push(async move {
                let metadata = self.metadata(&source_name).await?;
                Ok::<_, DependencyError>((source_name.as_str().to_string(), metadata))
            });
        }

        let concurrency = metadata_concurrency_limit();
        let mut source_results = std::collections::HashMap::new();
        let mut metadata_errors = Vec::new();
        for fetched in stream::iter(futures)
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await
        {
            match fetched {
                Ok((source_name, metadata)) => {
                    source_results.insert(source_name, metadata);
                }
                Err(e) => metadata_errors.push(e),
            }
        }
        if let Some(e) = metadata_errors.into_iter().next() {
            return Err(e);
        }

        for (alias_name, source_name) in alias_to_source {
            if results.contains_key(&alias_name) {
                continue;
            }
            if let Some(metadata) = source_results.get(source_name.as_str()) {
                results.insert(alias_name, Arc::clone(metadata));
            }
        }

        Ok(results)
    }

    pub fn version_key(package_id: &PackageId) -> String {
        format!("{}@{}", package_id.name_str(), package_id.version())
    }

    pub fn optional_enqueue_key(dep: &ResolvedDep) -> String {
        format!("{}@{}", dep.package.as_str(), dep.spec)
    }

    pub fn current_npm_os() -> &'static str {
        match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "win32",
            other => other,
        }
    }

    pub fn current_npm_cpu() -> &'static str {
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            "x86" => "ia32",
            "powerpc64" => "ppc64",
            "loongarch64" => "loong64",
            other => other,
        }
    }

    pub fn platform_matches(rules: Option<&[String]>, current: &str) -> bool {
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

    pub fn version_supported(info: &native::npm_registry::VersionInfo) -> bool {
        Self::platform_matches(info.os.as_deref(), Self::current_npm_os())
            && Self::platform_matches(info.cpu.as_deref(), Self::current_npm_cpu())
    }

    pub fn known_optional_native_binary_supported(package: &PackageName) -> Option<bool> {
        let name = package.as_str();
        let os = Self::current_npm_os();
        let cpu = Self::current_npm_cpu();
        let expected = format!("{os}-{cpu}");

        let target = name
            .strip_prefix("@esbuild/")
            .or_else(|| name.strip_prefix("@next/swc-"))
            .or_else(|| name.strip_prefix("@swc/core-"))
            .or_else(|| name.strip_prefix("@rollup/rollup-"))
            .or_else(|| name.strip_prefix("@tailwindcss/oxide-"))
            .or_else(|| name.strip_prefix("lightningcss-"))
            .or_else(|| name.strip_prefix("@parcel/watcher-"))?;

        Some(target.starts_with(&expected))
    }

    pub fn select_best_version(
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

    pub fn metadata_versions(metadata: &native::npm_registry::PackageMetadata) -> Vec<Version> {
        metadata
            .versions
            .keys()
            .filter_map(|v| Version::parse(v).ok())
            .collect()
    }
}

#[async_trait]
impl DependencyProvider for NpmDependencyProvider {
    async fn get_versions(&self, package: &PackageName) -> Result<Vec<Version>, DependencyError> {
        if let Some(cached) = self.cached_versions_for(package) {
            return Ok(cached);
        }
        let meta = self.metadata(package).await?;
        let v = Self::metadata_versions(&meta);
        self.insert_versions_for(package, v.clone());
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
                let mut collected =
                    self.collect_resolved_deps(v.dependencies.as_ref(), false, false);
                collected.extend(self.collect_resolved_deps(
                    v.optional_dependencies.as_ref(),
                    true,
                    false,
                ));
                collected.extend(self.collect_resolved_deps(
                    v.peer_dependencies.as_ref(),
                    false,
                    true,
                ));
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

        let cache_key = Self::optional_enqueue_key(dep);
        if let Some(cached) = self.optional_enqueue_cache.get(&cache_key) {
            return Ok(*cached);
        }

        if let Some(supported) = Self::known_optional_native_binary_supported(&dep.package) {
            self.optional_enqueue_cache.insert(cache_key, supported);
            return Ok(supported);
        }

        let meta = self.metadata(&dep.package).await?;
        let versions = Self::metadata_versions(&meta);
        self.insert_versions_for(&dep.package, versions.clone());
        let Some(selected) = Self::select_best_version(&versions, &dep.spec)? else {
            self.optional_enqueue_cache.insert(cache_key, false);
            return Ok(false);
        };
        let Some(info) = meta.versions.get(&selected.to_string()) else {
            self.optional_enqueue_cache.insert(cache_key, false);
            return Ok(false);
        };

        let supported = Self::version_supported(info);
        self.optional_enqueue_cache.insert(cache_key, supported);
        Ok(supported)
    }

    async fn prefetch_versions(
        &self,
        packages: &[PackageName],
    ) -> Result<Vec<(PackageName, Vec<Version>)>, DependencyError> {
        let mut results = Vec::with_capacity(packages.len());
        let mut missing = Vec::new();

        for package in packages {
            if let Some(cached) = self.cached_versions_for(package) {
                results.push((package.clone(), cached));
                continue;
            }

            let package_key = self.source_package_name(package).as_str().to_string();
            if let Some(metadata) = self.metadata_cache.get(&package_key) {
                let versions = Self::metadata_versions(&metadata);
                self.insert_versions_for(package, versions.clone());
                results.push((package.clone(), versions));
                continue;
            }
            missing.push(package.clone());
        }

        if missing.is_empty() {
            return Ok(results);
        }

        let fetched_metadata = self.prefetch_resolution_metadata(&missing).await?;
        for package in missing {
            let Some(metadata) = fetched_metadata.get(package.as_str()) else {
                return Err(DependencyError(format!(
                    "prefetch metadata missing result for '{}'",
                    package.as_str()
                )));
            };
            let versions = Self::metadata_versions(metadata);
            self.insert_versions_for(&package, versions.clone());
            results.push((package.clone(), versions));
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
                            self.collect_resolved_deps(v.dependencies.as_ref(), false, false);
                        collected.extend(self.collect_resolved_deps(
                            v.optional_dependencies.as_ref(),
                            true,
                            false,
                        ));
                        collected.extend(self.collect_resolved_deps(
                            v.peer_dependencies.as_ref(),
                            false,
                            true,
                        ));
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

        let missing_names: Vec<PackageName> =
            preloaded.iter().map(|id| id.name().clone()).collect();
        let fetched_metadata = self.prefetch_resolution_metadata(&missing_names).await?;

        for package_id in preloaded {
            let source_name = self.source_package_name(package_id.name());
            let Some(meta) = fetched_metadata.get(source_name.as_str()) else {
                return Err(DependencyError(format!(
                    "prefetch metadata missing result for '{}'",
                    package_id.name_str()
                )));
            };
            let deps = meta
                .versions
                .get(&package_id.version().to_string())
                .map(|v| {
                    let mut collected =
                        self.collect_resolved_deps(v.dependencies.as_ref(), false, false);
                    collected.extend(self.collect_resolved_deps(
                        v.optional_dependencies.as_ref(),
                        true,
                        false,
                    ));
                    collected.extend(self.collect_resolved_deps(
                        v.peer_dependencies.as_ref(),
                        false,
                        true,
                    ));
                    collected
                })
                .unwrap_or_default();
            self.registry_cache
                .insert_deps(Self::version_key(&package_id), deps.clone());
            results.push((package_id, deps));
        }

        Ok(results)
    }

    async fn on_batch_resolved(&self, _ids: &[PackageId]) -> Result<(), DependencyError> {
        Ok(())
    }
}
