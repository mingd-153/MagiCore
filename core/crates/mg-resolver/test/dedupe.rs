#![allow(clippy::unwrap_used)]
//! Dedupe preference tests — DedupePref/PreferExisting + peer merge (02 §2.1)
//! (Test ưu tiên version có sẵn trong lockfile khi prefer-dedupe)
use async_trait::async_trait;
use mg_resolver::solver::{DedupePref, DependencyError, DependencyProvider, ResolvedDep, Resolver};
use mg_types::{PackageId, PackageName, Version};
use std::collections::HashMap;
use std::sync::Arc;

struct VersionsProvider {
    versions: Vec<&'static str>,
}

#[async_trait]
impl DependencyProvider for VersionsProvider {
    async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
        Ok(self
            .versions
            .iter()
            .map(|s| Version::parse(s).unwrap())
            .collect())
    }

    async fn get_dependencies(&self, _: &PackageId) -> Result<Vec<ResolvedDep>, DependencyError> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn prefer_existing_reuses_installed_version_over_latest() {
    let provider = Arc::new(VersionsProvider {
        versions: vec!["1.0.0", "2.0.0"],
    });
    let resolver = Resolver::new(provider);
    resolver.set_dedupe_pref(DedupePref::PreferExisting);
    let mut existing = HashMap::new();
    existing.insert("react".to_string(), Version::parse("1.0.0").unwrap());
    resolver.set_existing_versions(existing);

    let wanted = vec![(PackageName::new("react").unwrap(), "^1.0.0".to_string())];
    let result = resolver.solve(&wanted).await.unwrap();
    assert_eq!(result.resolutions[0].version.to_string(), "1.0.0");
}

#[tokio::test]
async fn prefer_existing_ignores_installed_version_outside_range() {
    let provider = Arc::new(VersionsProvider {
        versions: vec!["1.0.0", "2.0.0"],
    });
    let resolver = Resolver::new(provider);
    resolver.set_dedupe_pref(DedupePref::PreferExisting);
    let mut existing = HashMap::new();
    existing.insert("react".to_string(), Version::parse("1.0.0").unwrap());
    resolver.set_existing_versions(existing);

    // Range ^2.0.0 — installed 1.0.0 does NOT match → must pick 2.0.0.
    let wanted = vec![(PackageName::new("react").unwrap(), "^2.0.0".to_string())];
    let result = resolver.solve(&wanted).await.unwrap();
    assert_eq!(result.resolutions[0].version.to_string(), "2.0.0");
}

#[tokio::test]
async fn prefer_latest_default_picks_newest() {
    let provider = Arc::new(VersionsProvider {
        versions: vec!["1.0.0", "1.2.0"],
    });
    let resolver = Resolver::new(provider);
    let wanted = vec![(PackageName::new("react").unwrap(), "^1.0.0".to_string())];
    let result = resolver.solve(&wanted).await.unwrap();
    assert_eq!(result.resolutions[0].version.to_string(), "1.2.0");
}
