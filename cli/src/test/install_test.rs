#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for install command validation

use super::*;
use mgc_lockfile::Package;
use mgc_types::{DependencySpec, Ecosystem, PackageName, VersionRange};
use tempfile::tempdir;

#[test]
fn test_lock_matches_manifest_when_versions_satisfy_ranges() {
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
    lock.packages.push(Package {
        name: "tailwindcss".into(),
        version: "4.3.2".into(),
        resolved: "https://registry.npmjs.org/tailwindcss/-/tailwindcss-4.3.2.tgz".into(),
        integrity: "sha256-test".into(),
        dependencies: vec![],
    });

    assert!(lock_matches_manifest(&lock, &manifest));
}

#[test]
fn test_lock_matches_manifest_rejects_stale_version() {
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
    lock.packages.push(Package {
        name: "tailwindcss".into(),
        version: "4.3.2".into(),
        resolved: "https://registry.npmjs.org/tailwindcss/-/tailwindcss-4.3.2.tgz".into(),
        integrity: "sha256-test".into(),
        dependencies: vec![],
    });

    assert!(!lock_matches_manifest(&lock, &manifest));
}

#[test]
fn test_load_locked_graph_rejects_unsupported_lock_version() {
    let dir = tempdir().unwrap();
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
    lock.version = "0".into(); // Unsupported version
    lock.packages.push(Package {
        name: "tailwindcss".into(),
        version: "4.3.2".into(),
        resolved: "https://registry.npmjs.org/tailwindcss/-/tailwindcss-4.3.2.tgz".into(),
        integrity: "sha256-test".into(),
        dependencies: vec![],
    });
    std::fs::write(
        dir.path().join("mgc.lock"),
        mgc_lockfile::serialization::to_toml(&lock).unwrap(),
    )
    .unwrap();

    let err = load_locked_graph(dir.path(), "web", &manifest).unwrap_err();
    assert!(err.to_string().contains("unsupported lockfile version"));
}

#[test]
fn test_load_locked_graph_ignores_legacy_checksum_sidecar() {
    // Set trust policy to warn for test (no signature required)
    std::env::set_var("MGC_TRUST_POLICY", "warn");

    let dir = tempdir().unwrap();
    let manifest = Manifest::new("demo", Ecosystem::Web);
    let lock = Lockfile::new();
    std::fs::write(
        dir.path().join("mgc.lock"),
        mgc_lockfile::serialization::to_toml(&lock).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("mgc.lock.sha256"), "bad").unwrap();

    assert!(load_locked_graph(dir.path(), "web", &manifest)
        .unwrap()
        .is_none());
}

#[test]
fn test_graph_from_lockfile_rejects_invalid_dependency_id() {
    let mut lock = Lockfile::new();
    lock.packages.push(Package {
        name: "react".into(),
        version: "18.2.0".into(),
        resolved: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".into(),
        integrity: "sha256-test".into(),
        dependencies: vec!["not-a-package-id".into()],
    });

    let err = graph_from_lockfile(&lock).unwrap_err();

    assert!(
        err.to_string().contains("invalid package spec"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_discover_workspace_projects_for_monorepo_root() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("magicore.workspace.toml"),
        r#"
version = 1
mode = "monorepo"

[layout]
apps_dir = "apps"
packages_dir = "packages"
"#,
    )
    .unwrap();

    let frontend = dir.path().join("apps").join("frontend");
    let backend = dir.path().join("apps").join("backend");
    let contracts = dir.path().join("packages").join("contracts");
    fs::create_dir_all(&frontend).unwrap();
    fs::create_dir_all(&backend).unwrap();
    fs::create_dir_all(&contracts).unwrap();
    fs::write(frontend.join("package.json"), "{}").unwrap();
    fs::write(contracts.join("package.json"), "{}").unwrap();

    let workspaces = discover_workspace_projects(dir.path())
        .unwrap()
        .expect("should detect monorepo");

    assert_eq!(workspaces, vec![frontend, contracts]);
    assert!(!workspaces.contains(&backend));
}

#[test]
fn test_discover_workspace_projects_mix_cores() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("magicore.workspace.toml"),
        r#"
mode = "monorepo"
[layout]
apps_dir = "apps"
packages_dir = "packages"
"#,
    )
    .unwrap();

    let web = dir.path().join("apps/web");
    fs::create_dir_all(&web).unwrap();
    fs::write(web.join("package.json"), "{}").unwrap();

    let lib = dir.path().join("packages/rustlib");
    fs::create_dir_all(lib.join("src")).unwrap();
    fs::write(lib.join("Cargo.toml"), "[package]\nname = \"rustlib\"\n").unwrap();

    let ignored = dir.path().join("packages/not-a-project");
    fs::create_dir_all(&ignored).unwrap();
    fs::write(ignored.join("notes.txt"), "x").unwrap();

    let mut workspaces = discover_workspace_projects(dir.path()).unwrap().unwrap();
    workspaces.sort();
    let normalized: Vec<String> = workspaces
        .iter()
        .map(|p| {
            p.strip_prefix(dir.path())
                .unwrap_or(p)
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(normalized, vec!["apps/web", "packages/rustlib"]);
}

#[test]
fn test_discover_workspace_projects_ignores_non_monorepo_file() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("magicore.workspace.toml"),
        r#"
version = 1
mode = "single"
"#,
    )
    .unwrap();

    assert!(discover_workspace_projects(dir.path()).unwrap().is_none());
}
