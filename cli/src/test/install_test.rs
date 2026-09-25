#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for install command validation

use super::*;
use crate::commands::core::shared::lock_matches_manifest;
use mgc_crypto::keyring::KeyPair;
use mgc_lockfile::Package;
use mgc_types::adapter::PackageAdapter;
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
        ecosystem: mgc_lockfile::EcosystemTag::Web,
        registry: Some("npm://https://registry.npmjs.org".into()),
        ..Package::default()
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
        ecosystem: mgc_lockfile::EcosystemTag::Web,
        registry: Some("npm://https://registry.npmjs.org".into()),
        ..Package::default()
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
        ecosystem: mgc_lockfile::EcosystemTag::Web,
        registry: Some("npm://https://registry.npmjs.org".into()),
        ..Package::default()
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
// SAFETY (edition 2024): `env::set_var` is unsafe — this test mutates
// MGC_TRUST_POLICY inside its own test process, matching the repo-wide
// test env convention (chain_test.rs / npmrc.rs / auth.rs).
// (An toàn (edition 2024): `env::set_var` là unsafe — test đổi
// MGC_TRUST_POLICY trong process test riêng, theo đúng quy ước env của
// test toàn repo (chain_test.rs / npmrc.rs / auth.rs).)
#[allow(unsafe_code)]
fn test_load_locked_graph_ignores_legacy_checksum_sidecar() {
    // Set trust policy to warn for test (no signature required)
    unsafe { std::env::set_var("MGC_TRUST_POLICY", "warn") };

    let dir = tempdir().unwrap();
    let manifest = Manifest::new("demo", Ecosystem::Web);
    let mut lock = Lockfile::new();
    let lock_path = dir.path().join("mgc.lock");
    let key = KeyPair::generate().unwrap();
    mgc_lockfile::sign_and_write_lockfile(&mut lock, &lock_path, &key).unwrap();
    std::fs::write(dir.path().join("mgc.lock.sha256"), "bad").unwrap();

    assert!(
        load_locked_graph(dir.path(), "web", &manifest)
            .unwrap()
            .is_none()
    );
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
        ecosystem: mgc_lockfile::EcosystemTag::Web,
        registry: Some("npm://https://registry.npmjs.org".into()),
        ..Package::default()
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

#[cfg(feature = "clo")]
#[test]
fn clo_adapter_path_terraform_gates_without_compat() {
    // Terraform is unsupported until MGC owns its dependency lifecycle;
    // an absent compatibility runner must not be presented as opt-in support.
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("main.tf"), "terraform {}\n").unwrap();
    let err = clo_adapter_path_gate(dir.path()).unwrap_err();
    assert!(
        err.to_string()
            .contains("unsupported for dependency lifecycle"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "clo")]
#[test]
fn clo_adapter_path_cdk_skips_gate_natively() {
    // CDK rides the native web engine inside the adapter — no gate, no
    // compat needed.
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.toml"),
        "name = \"x\"\n[cloud]\ntype = \"cdk\"\n",
    )
    .unwrap();
    clo_adapter_path_gate(dir.path()).unwrap();
}

#[cfg(feature = "clo")]
#[test]
fn clo_adapter_path_undetected_type_fails_closed() {
    // No detectable cloud type is never assumed native.
    let dir = tempdir().unwrap();
    let err = clo_adapter_path_gate(dir.path()).unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[cfg(feature = "iot")]
#[test]
fn toolchain_owned_packages_rejected_before_adapter_calls() {
    // P0-1: platformio.ini (toolchain-owned) + packages fails PURELY —
    // no adapter call, no network, no spawn, no journal. The provider
    // toolchain must run explicitly.
    // (Manifest của tool + packages → lỗi thuần, không side effect.)
    let dir = tempdir().unwrap();
    let ini = "[env:test]\nplatform = atmelavr\nframework = arduino\n";
    std::fs::write(dir.path().join("platformio.ini"), ini).unwrap();
    let adapter =
        mgc_iot_adapter::adapter_for(dir.path()).expect("platformio must detect an iot adapter");
    assert!(
        !adapter.manifest_owned(),
        "platformio.ini must be toolchain-owned"
    );
    let err = reject_toolchain_owned_packages(&adapter, &["some-pkg".to_string()]).unwrap_err();
    assert!(
        format!("{err:#}").contains("does not own its native dependency lifecycle"),
        "must name the ownership reason: {err:#}"
    );
    assert_eq!(
        std::fs::read(dir.path().join("platformio.ini")).unwrap(),
        ini.as_bytes(),
        "manifest must be byte-identical (zero side effects)"
    );
    assert!(
        !dir.path()
            .join(".magicore/journal/dependency-mutation")
            .exists(),
        "no journal may be staged by a rejected op"
    );
    // Empty packages on the same lane stays allowed (pure install path).
    // (Không packages thì qua — đường install thuần.)
    reject_toolchain_owned_packages(&adapter, &[]).unwrap();
}

#[cfg(feature = "game")]
#[test]
fn game_owner_preflight_bevy_passes_godot_fails() {
    // P1-1: preflight dùng engine detect thật — bevy qua gate Native,
    // godot rớt catch-all Unsupported (không bao giờ đánh giá bằng
    // ownership của Bevy).
    // (Preflight uses the detected engine — godot fails closed.)
    let bevy = tempdir().unwrap();
    std::fs::write(
        bevy.path().join("mgc.toml"),
        "name = \"g\"\necosystem = \"game\"\n[game]\nengine = \"bevy\"\n",
    )
    .unwrap();
    std::fs::write(
        bevy.path().join("Cargo.toml"),
        "[package]\nname = \"g\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let bevy_adapter =
        mgc_game_adapter::adapter_for(bevy.path()).expect("bevy must detect a game adapter");
    validate_install_owner(&bevy_adapter, bevy.path()).unwrap();

    let godot = tempdir().unwrap();
    std::fs::write(godot.path().join("project.godot"), "; godot\n").unwrap();
    let godot_adapter =
        mgc_game_adapter::adapter_for(godot.path()).expect("godot must detect a game adapter");
    validate_install_owner(&godot_adapter, godot.path())
        .expect_err("godot must fail the ownership gate (Unsupported, never Bevy's cell)");
}
