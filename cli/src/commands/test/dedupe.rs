use super::merged_lockfile;
use mgc_lockfile::{EcosystemTag, Lockfile, Package};

fn package(owner: &str, ecosystem: EcosystemTag) -> Package {
    Package {
        owner_core: Some(owner.to_string()),
        name: "shared-name".to_string(),
        version: "1.0.0".to_string(),
        resolved: "https://registry.example/shared-name-1.0.0.tgz".to_string(),
        integrity: "sha512-identical".to_string(),
        ecosystem,
        registry: Some("https://registry.example".to_string()),
        ..Default::default()
    }
}

#[test]
fn dedupe_keeps_identical_name_version_owned_by_distinct_cores() {
    let mut lock = Lockfile::new();
    lock.packages = vec![
        package("ai", EcosystemTag::Python),
        package("lib", EcosystemTag::Python),
        package("web", EcosystemTag::Web),
    ];

    let (deduped, merged) = merged_lockfile(&lock).unwrap();

    assert_eq!(merged, 0);
    assert_eq!(deduped.packages.len(), 3);
}

#[test]
fn dedupe_collapses_only_exact_same_owned_lock_records() {
    let duplicate = package("web", EcosystemTag::Web);
    let mut lock = Lockfile::new();
    lock.packages = vec![duplicate.clone(), duplicate];

    let (deduped, merged) = merged_lockfile(&lock).unwrap();

    assert_eq!(merged, 1);
    assert_eq!(deduped.packages.len(), 1);
}
