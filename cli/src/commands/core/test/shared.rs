use super::*;
use mgc_lockfile::{Lockfile, Package};
use mgc_types::{DependencySpec, Ecosystem, PackageName, VersionRange};

#[test]
fn v2_lock_matches_manifest_when_locked_version_satisfies_range() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("tailwindcss").unwrap(),
            VersionRange::parse("^4.3.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "tailwindcss".into(),
        "4.3.2".into(),
        "https://registry.example/tailwindcss.tgz".into(),
        "blake3-tailwindcss".into(),
    ));

    assert!(lock_matches_manifest(&lock, &manifest));
}

#[test]
fn v2_lock_rejects_stale_manifest_requirement() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("tailwindcss").unwrap(),
            VersionRange::parse("^5.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "tailwindcss".into(),
        "4.3.2".into(),
        "https://registry.example/tailwindcss.tgz".into(),
        "blake3-tailwindcss".into(),
    ));

    assert!(!lock_matches_manifest(&lock, &manifest));
}

#[test]
fn v2_graph_preserves_dependency_edges() {
    let mut lock = Lockfile::new();
    let mut react = Package::new(
        "react".into(),
        "19.2.0".into(),
        "https://registry.example/react.tgz".into(),
        "blake3-react".into(),
    );
    react.add_dependency("scheduler@0.25.0".into());
    lock.add_package(react);
    lock.add_package(Package::new(
        "scheduler".into(),
        "0.25.0".into(),
        "https://registry.example/scheduler.tgz".into(),
        "blake3-scheduler".into(),
    ));

    let graph = graph_from_lockfile(&lock).unwrap();
    assert_eq!(graph.packages.len(), 2);
    assert_eq!(graph.packages[0].deps[0].to_string(), "scheduler@0.25.0");
}
