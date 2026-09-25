//! `provider.rs` — `NpmDependencyProvider` implementation for `mgc-resolver`.

use async_trait::async_trait;
use dashmap::DashMap;
use futures_util::stream::{self, StreamExt};
use mgc_resolver::{DependencyError, DependencyProvider, RegistryCache, ResolvedDep};
use mgc_types::{PackageId, PackageName, Version, VersionRange};
use std::sync::Arc;

use crate::cache::{
    MetadataCache, SharedWebCache, load_metadata_with_fallback, metadata_concurrency_limit,
};
use crate::native;

/// Age-gate policy: cutoff plus the missing-timestamp rule. Missing or
/// unparsable timestamps REJECT the version by default (fail-closed —
/// a policy that silently keeps unstamped versions is a bypass); the
/// explicit `allow_missing_time` escape hatch exists for private
/// registries that omit `time`.
/// (Policy cổng tuổi: thiếu timestamp thì reject, trừ escape hatch.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgePolicy {
    /// Minimum age in hours.
    pub cutoff_hours: u64,
    /// Keep versions with missing/unparsable timestamps (private
    /// registries). Default false.
    pub allow_missing_time: bool,
}

/// Parse npm `time` values (`2026-09-18T12:00:00.000Z` or without millis)
/// to unix seconds. Returns None when unparseable (caller keeps version).
/// (Parse thời gian npm ra unix seconds.)
pub(crate) fn parse_npm_time(raw: &str) -> Option<u64> {
    let normalized = raw.trim_end_matches('Z');
    let (date, time) = normalized.split_once('T')?;
    let mut date_parts = date.split('-');
    let (y, m, d) = (
        date_parts.next()?.parse::<u64>().ok()?,
        date_parts.next()?.parse::<u64>().ok()?,
        date_parts.next()?.parse::<u64>().ok()?,
    );
    let time = time.split('.').next()?;
    let mut time_parts = time.split(':');
    let (hh, mm, ss) = (
        time_parts.next()?.parse::<u64>().ok()?,
        time_parts.next()?.parse::<u64>().ok()?,
        time_parts.next()?.parse::<u64>().ok()?,
    );
    if !(1..=12).contains(&m) || d == 0 || d > 31 || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Days-from-civil (Howard Hinnant) — no chrono dependency here.
    // (Ngày từ dân sự — không cần chrono.)
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = y_adj / 400;
    let yoe = y_adj - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

pub struct NpmDependencyProvider {
    pub registry: native::npm_registry::NpmRegistry,
    pub metadata_cache: MetadataCache,
    pub metadata_locks: DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    pub registry_cache: RegistryCache,
    pub shared_cache: Option<SharedWebCache>,
    pub alias_targets: DashMap<String, PackageName>,
    pub optional_enqueue_cache: DashMap<String, bool>,
    /// Per-instance age gate (P0/F6): armed per operation from that
    /// operation's project — never a process-global first-wins.
    /// (Cổng tuổi riêng từng instance.)
    age_policy: std::sync::RwLock<Option<AgePolicy>>,
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
            age_policy: std::sync::RwLock::new(None),
        }
    }

    /// Overwrite this provider's gate (P0/F6 — every operation re-arms
    /// from its own project; no first-writer-wins). Also flips the
    /// registry client's fetch mode (full packuments + no stale
    /// abbreviated cache) so fetchers and filters can never disagree.
    /// (Đặt lại cổng tuổi — mỗi operation nạp lại từ project của nó.)
    pub fn set_age_policy(&self, policy: Option<AgePolicy>) {
        let armed = policy.is_some_and(|p| p.cutoff_hours > 0);
        if let Ok(mut slot) = self.age_policy.write() {
            *slot = policy;
        }
        self.registry.set_age_gate_armed(armed);
    }

    /// This provider's current policy (None = no filtering).
    /// (Policy hiện tại của provider này.)
    pub fn age_policy(&self) -> Option<AgePolicy> {
        self.age_policy.read().ok().and_then(|p| *p)
    }

    /// Test/scope probe: is any cutoff armed on this instance.
    /// (Probe: instance này có cutoff không.)
    pub fn age_gate_armed_for_test(&self) -> bool {
        self.age_policy().is_some_and(|p| p.cutoff_hours > 0)
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

        for (alias_name, source_name) in &alias_to_source {
            if results.contains_key(alias_name) {
                continue;
            }
            if let Some(metadata) = source_results.get(source_name.as_str()) {
                results.insert(alias_name.clone(), Arc::clone(metadata));
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

    /// Version list with the minimum-age gate applied — the ONLY place
    /// version lists are built (get_versions + all prefetch paths funnel
    /// here, so the gate cannot be bypassed by a second listing site).
    /// Gate rejections propagate as precise errors (never a confusing
    /// downstream "no version matches").
    /// (Dựng danh sách version kèm cổng tuổi — điểm duy nhất.)
    pub(crate) fn versions_with_age_gate(
        &self,
        package: &PackageName,
        meta: &native::npm_registry::PackageMetadata,
    ) -> Result<Vec<Version>, DependencyError> {
        eligible_versions(package, meta, self.age_policy()).map_err(DependencyError)
    }
}

/// Verify ONE explicitly selected version (dist-tag pin, add-latest,
/// update target) against the age policy. Unlike list filtering, there
/// is no fallback candidate — a rejected pin is a hard error telling
/// the user to pin an older version explicitly.
/// (Kiểm tra một version đã chọn theo policy tuổi.)
pub fn check_pinned_version(
    package: &PackageName,
    meta: &native::npm_registry::PackageMetadata,
    version: &str,
    policy: Option<AgePolicy>,
) -> Result<(), String> {
    let Some(policy) = policy.filter(|p| p.cutoff_hours > 0) else {
        return Ok(());
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match meta.time.get(version).and_then(|text| parse_npm_time(text)) {
        Some(ts) if now.saturating_sub(ts) >= policy.cutoff_hours.saturating_mul(3600) => Ok(()),
        Some(_) => Err(format!(
            "minimum-release-age: {}@{} is younger than {}h — pin an older version explicitly or relax [security] min_release_age",
            package.as_str(),
            version,
            policy.cutoff_hours
        )),
        None if policy.allow_missing_time => Ok(()),
        None => Err(format!(
            "minimum-release-age: {}@{} has no registry timestamp — set [security] allow_missing_time for private registries",
            package.as_str(),
            version
        )),
    }
}
/// and every selection path share it): resolve, prefetch, dist-tag
/// pinning, add-latest, update, outdated. `None` policy = historical
/// behavior (all parsed versions). With a policy, young versions are
/// excluded and unstamped versions are excluded unless the explicit
/// `allow_missing_time` escape hatch; empty-after-filter on a non-empty
/// candidate set is an explicit Err naming the policy.
/// (Hàm eligible duy nhất — mọi đường chọn version đi qua đây.)
pub fn eligible_versions(
    package: &PackageName,
    meta: &native::npm_registry::PackageMetadata,
    policy: Option<AgePolicy>,
) -> Result<Vec<Version>, String> {
    let all: Vec<Version> = meta
        .versions
        .keys()
        .filter_map(|v| Version::parse(v).ok())
        .collect();
    let Some(policy) = policy.filter(|p| p.cutoff_hours > 0) else {
        return Ok(all);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut kept = Vec::new();
    let mut young = 0usize;
    let mut unstamped = 0usize;
    for version in all {
        let key = version.to_string();
        match meta.time.get(&key).and_then(|text| parse_npm_time(text)) {
            Some(ts) if now.saturating_sub(ts) >= policy.cutoff_hours.saturating_mul(3600) => {
                kept.push(version);
            }
            Some(_) => {
                young += 1;
            }
            None => {
                if policy.allow_missing_time {
                    kept.push(version);
                } else {
                    unstamped += 1;
                }
            }
        }
    }
    if kept.is_empty() && !meta.versions.is_empty() {
        return Err(format!(
            "minimum-release-age: all {} version(s) of {} excluded ({} too young, {} unstamped) — relax [security] min_release_age, set allow_missing_time, or pin an older version explicitly",
            meta.versions.len(),
            package.as_str(),
            young,
            unstamped
        ));
    }
    if young > 0 || unstamped > 0 {
        eprintln!(
            "[magicore] minimum-release-age: excluded {} young + {} unstamped version(s) of {} (cutoff {}h)",
            young,
            unstamped,
            package.as_str(),
            policy.cutoff_hours
        );
    }
    Ok(kept)
}

#[async_trait]
impl DependencyProvider for NpmDependencyProvider {
    async fn get_versions(&self, package: &PackageName) -> Result<Vec<Version>, DependencyError> {
        if let Some(cached) = self.cached_versions_for(package) {
            return Ok(cached);
        }
        let meta = self.metadata(package).await?;
        let v = self.versions_with_age_gate(package, &meta)?;
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
        let versions = self.versions_with_age_gate(&dep.package, &meta)?;
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
                let versions = self.versions_with_age_gate(package, &metadata)?;
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
            let versions = self.versions_with_age_gate(&package, metadata)?;
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
            // Results are keyed by ALIAS (the name edges request); fall
            // back to the source name for unaliased packages. Looking up
            // by source only broke every real npm: alias
            // (e.g. `vite → @voidzero-dev/vite-plus-core` in Nuxt's tree).
            // (Kết quả key theo ALIAS; fallback tên gốc.)
            let source_name = self.source_package_name(package_id.name());
            let Some(meta) = fetched_metadata
                .get(package_id.name_str())
                .or_else(|| fetched_metadata.get(source_name.as_str()))
            else {
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
