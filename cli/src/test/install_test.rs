#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for install command validation

use super::*;
use crate::commands::core::shared::{lock_graph_matches_manifest_closure, lock_matches_manifest};
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
fn locked_graph_must_be_exactly_reachable_from_manifest_roots() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("root-pkg").unwrap(),
            VersionRange::parse("^1.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let mut lock = Lockfile::new();
    lock.packages = vec![
        Package {
            name: "root-pkg".into(),
            version: "1.0.0".into(),
            dependencies: vec!["child-pkg@2.0.0".into()],
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some("web".into()),
            ..Package::default()
        },
        Package {
            name: "child-pkg".into(),
            version: "2.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some("web".into()),
            ..Package::default()
        },
        Package {
            name: "orphan-pkg".into(),
            version: "9.9.9".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some("web".into()),
            ..Package::default()
        },
    ];

    assert!(
        !lock_graph_matches_manifest_closure(&lock, &manifest, "web"),
        "orphan entries must not be trusted as part of the install graph"
    );
    lock.packages.pop();
    lock.root_dependencies_by_owner
        .insert("web".into(), vec!["root-pkg@1.0.0".into()]);
    assert!(lock_graph_matches_manifest_closure(&lock, &manifest, "web"));
}

#[test]
fn locked_graph_uses_owner_scoped_exact_roots_and_peer_edges() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("root-pkg").unwrap(),
            VersionRange::parse("^1.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let owner = "web";
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner
        .insert(owner.into(), vec!["root-pkg@1.0.0".into()]);
    lock.packages = vec![
        Package {
            name: "root-pkg".into(),
            version: "1.0.0".into(),
            dependencies: vec!["bridge@1.0.0".into()],
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
        Package {
            name: "bridge".into(),
            version: "1.0.0".into(),
            dependencies: vec!["root-pkg@1.5.0".into()],
            peers: Some(vec!["peer-pkg@2.0.0".into()]),
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
        Package {
            name: "root-pkg".into(),
            version: "1.5.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
        Package {
            name: "peer-pkg".into(),
            version: "2.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
    ];

    assert!(
        lock_graph_matches_manifest_closure(&lock, &manifest, owner),
        "exact owner root must coexist with another satisfying transitive version and peer edge"
    );
    lock.packages[1].peers = None;
    assert!(
        !lock_graph_matches_manifest_closure(&lock, &manifest, owner),
        "a peer package omitted from graph edges must not be silently trusted"
    );
}

#[test]
fn locked_graph_ignores_unrelated_core_entries_in_unified_lock() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("web-root").unwrap(),
            VersionRange::parse("^1.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner
        .insert("web".into(), vec!["web-root@1.0.0".into()]);
    lock.packages = vec![
        Package {
            name: "web-root".into(),
            version: "1.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Web,
            owner_core: Some("web".into()),
            ..Package::default()
        },
        Package {
            name: "app-root".into(),
            version: "2.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Dart,
            owner_core: Some("app".into()),
            ..Package::default()
        },
    ];

    assert!(lock_graph_matches_manifest_closure(&lock, &manifest, "web"));
}

#[test]
fn locked_graph_scopes_roots_to_selected_ecosystem_for_one_owner() {
    let mut manifest = Manifest::new("demo", Ecosystem::App);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("shared-name").unwrap(),
            VersionRange::parse("^1.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let owner = "app";
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner.insert(
        owner.into(),
        vec![
            mgc_lockfile::format_root_pin(mgc_lockfile::EcosystemTag::Dart, "shared-name@1.0.0"),
            mgc_lockfile::format_root_pin(mgc_lockfile::EcosystemTag::Swift, "swift-root@2.0.0"),
        ],
    );
    lock.packages = vec![
        Package {
            name: "shared-name".into(),
            version: "1.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Dart,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
        Package {
            name: "swift-root".into(),
            version: "2.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Swift,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
    ];

    assert!(lock_graph_matches_manifest_closure(&lock, &manifest, owner));
}

#[test]
fn ecosystem_qualified_root_disambiguates_identical_package_ids() {
    let mut manifest = Manifest::new("demo", Ecosystem::App);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("shared-name").unwrap(),
            VersionRange::parse("^1.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let owner = "app";
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner.insert(
        owner.into(),
        vec![mgc_lockfile::format_root_pin(
            mgc_lockfile::EcosystemTag::Dart,
            "shared-name@1.0.0",
        )],
    );
    lock.packages = vec![
        Package {
            name: "shared-name".into(),
            version: "1.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Dart,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
        Package {
            name: "shared-name".into(),
            version: "1.0.0".into(),
            ecosystem: mgc_lockfile::EcosystemTag::Swift,
            owner_core: Some(owner.into()),
            ..Package::default()
        },
    ];

    assert!(lock_graph_matches_manifest_closure(&lock, &manifest, owner));
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
fn test_load_locked_graph_ignores_legacy_checksum_sidecar() {
    let dir = tempdir().unwrap();
    let manifest = Manifest::new("demo", Ecosystem::Web);
    let mut lock = Lockfile::new();
    let lock_path = dir.path().join("mgc.lock");
    let key = KeyPair::generate().unwrap();
    mgc_lockfile::sign_and_write_lockfile(&mut lock, &lock_path, &key).unwrap();
    std::fs::write(dir.path().join("mgc.lock.sha256"), "bad").unwrap();
    std::fs::write(
        dir.path().join("mgc.toml"),
        format!(
            "name = \"demo\"\necosystem = \"web\"\n[lock]\npolicy = \"require\"\n[trust]\nkeys = [\"{}\"]\n",
            key.key_id
        ),
    )
    .unwrap();

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
fn workspace_discovery_rejects_layout_paths_outside_project_root() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("project");
    let outside_package = dir.path().join("outside/package");
    fs::create_dir_all(&outside_package).unwrap();
    fs::write(outside_package.join("package.json"), "{}").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("magicore.workspace.toml"),
        "mode = \"monorepo\"\n[layout]\napps_dir = \"../outside\"\npackages_dir = \"packages\"\n",
    )
    .unwrap();

    let error = discover_workspace_projects(&project)
        .expect_err("workspace layout must not escape the project root");
    assert!(error.to_string().contains("workspace layout path"));
}

#[cfg(unix)]
#[test]
fn workspace_discovery_rejects_symlinked_layout_directory() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("project");
    let outside_package = dir.path().join("outside/package");
    fs::create_dir_all(&outside_package).unwrap();
    fs::write(outside_package.join("package.json"), "{}").unwrap();
    fs::create_dir_all(&project).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside"), project.join("apps")).unwrap();
    fs::write(
        project.join("magicore.workspace.toml"),
        "mode = \"monorepo\"\n[layout]\napps_dir = \"apps\"\npackages_dir = \"packages\"\n",
    )
    .unwrap();

    let error = discover_workspace_projects(&project)
        .expect_err("workspace discovery must not follow a symlinked layout directory");
    assert!(error.to_string().contains("symlink"));
}

#[cfg(unix)]
#[test]
fn workspace_discovery_rejects_symlinked_workspace_config() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let external_config = dir.path().join("external-workspace.toml");
    fs::write(&external_config, "mode = \"monorepo\"\n").unwrap();
    std::os::unix::fs::symlink(external_config, project.join("magicore.workspace.toml")).unwrap();

    let error = discover_workspace_projects(&project)
        .expect_err("workspace config symlink must not be followed");
    assert!(error.to_string().contains("symlink"));
}

#[test]
fn workspace_discovery_propagates_invalid_core_marker_instead_of_descending() {
    let dir = tempdir().unwrap();
    let invalid_project = dir.path().join("apps/invalid");
    let nested_project = invalid_project.join("nested");
    fs::create_dir_all(&nested_project).unwrap();
    fs::write(invalid_project.join(".mgc.core"), "not-a-core\n").unwrap();
    fs::write(nested_project.join("package.json"), "{}").unwrap();

    let mut discovered = Vec::new();
    let error = collect_installable_projects(dir.path().join("apps"), &mut discovered)
        .expect_err("an invalid core marker must abort workspace discovery");

    assert!(
        error.to_string().contains("invalid core marker")
            && !error.to_string().contains("signature")
    );
    assert!(
        discovered.is_empty(),
        "nested projects must not bypass invalid parent identity"
    );
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
    let err =
        clo_adapter_path_gate(dir.path(), crate::commands::dep_gate::DepOp::Install).unwrap_err();
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
    std::fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
    clo_adapter_path_gate(dir.path(), crate::commands::dep_gate::DepOp::Install).unwrap();
}

#[cfg(feature = "clo")]
#[test]
fn clo_adapter_path_undetected_type_fails_closed() {
    // No detectable cloud type is never assumed native.
    let dir = tempdir().unwrap();
    let err =
        clo_adapter_path_gate(dir.path(), crate::commands::dep_gate::DepOp::Install).unwrap_err();
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

#[cfg(feature = "lib")]
#[test]
fn generic_install_owner_accepts_native_maven_and_dotnet_manifests() {
    // The generic `mgc install` entry must carry the same manifest-format
    // ownership evidence as `mgc install-lib` for native Java/.NET lanes.
    // (Lệnh install tổng quát phải truyền bằng chứng manifest như install-lib.)
    let maven = tempdir().unwrap();
    std::fs::write(
        maven.path().join("pom.xml"),
        "<project><modelVersion>4.0.0</modelVersion></project>",
    )
    .unwrap();
    let maven_adapter = mgc_lib_adapter::adapter_for(maven.path(), None, None)
        .unwrap()
        .expect("Maven manifest must detect a library adapter");
    validate_install_owner(&maven_adapter, maven.path())
        .expect("native Maven POM must pass the generic install ownership gate");

    let dotnet = tempdir().unwrap();
    std::fs::write(
        dotnet.path().join("Demo.csproj"),
        "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    )
    .unwrap();
    let dotnet_adapter = mgc_lib_adapter::adapter_for(dotnet.path(), None, None)
        .unwrap()
        .expect("csproj must detect a library adapter");
    validate_install_owner(&dotnet_adapter, dotnet.path())
        .expect("native .NET csproj must pass the generic install ownership gate");

    let gradle = tempdir().unwrap();
    std::fs::write(gradle.path().join("build.gradle.kts"), "plugins {}\n").unwrap();
    let gradle_adapter = mgc_lib_adapter::adapter_for(gradle.path(), None, None)
        .unwrap()
        .expect("Gradle manifest must still detect a library adapter");
    validate_install_owner(&gradle_adapter, gradle.path())
        .expect_err("Gradle scripts must remain outside native Maven ownership");

    let ambiguous = tempdir().unwrap();
    std::fs::write(
        ambiguous.path().join("pom.xml"),
        "<project><modelVersion>4.0.0</modelVersion></project>",
    )
    .unwrap();
    std::fs::write(ambiguous.path().join("build.gradle"), "plugins {}\n").unwrap();
    let ambiguous_adapter = mgc_lib_adapter::adapter_for(ambiguous.path(), None, None)
        .unwrap()
        .expect("mixed Maven/Gradle project must detect a library adapter");
    validate_install_owner(&ambiguous_adapter, ambiguous.path())
        .expect_err("mixed Maven/Gradle ownership must fail closed");
}

#[cfg(feature = "ai")]
#[test]
fn generic_install_owner_recognizes_native_ai_pep621_lane() {
    let native = tempdir().unwrap();
    std::fs::write(
        native.path().join("pyproject.toml"),
        "[project]\nname = \"demo-agent\"\ndependencies = [\"six>=1.16\"]\n\n[tool.magicore]\nframework = \"python-agent\"\n",
    )
    .unwrap();
    std::fs::write(
        native.path().join("mgc.toml"),
        "name = \"demo-agent\"\necosystem = \"ai\"\n[ai]\nframework = \"python-agent\"\n",
    )
    .unwrap();
    let adapter =
        mgc_ai_adapter::adapter_for(native.path()).expect("AI pyproject should detect an adapter");
    assert!(
        adapter.python_lane.is_some(),
        "PEP 621 lane should be native"
    );
    validate_install_owner(&adapter, native.path())
        .expect("generic install must pass the same native AI lane as install-ai");
    validate_install_owner_for_op(
        &adapter,
        native.path(),
        crate::commands::dep_gate::DepOp::OfflineReinstall,
    )
    .expect_err("AI's online native install must not be advertised as offline-capable");

    std::fs::write(native.path().join("uv.lock"), "version = 1\n").unwrap();
    let foreign = mgc_ai_adapter::adapter_for(native.path())
        .expect("AI project with foreign lock should still detect");
    assert!(
        foreign.python_lane.is_none(),
        "foreign lock must disable native lane"
    );
    validate_install_owner(&foreign, native.path())
        .expect_err("generic install must not admit a Python project owned by uv.lock");
}

#[cfg(feature = "clo")]
#[test]
fn generic_install_owner_gates_cloudflare_and_cdk_by_real_manifest_route() {
    let cloudflare = tempdir().unwrap();
    std::fs::write(cloudflare.path().join("wrangler.toml"), "name = 'demo'\n").unwrap();
    let cloudflare_adapter = mgc_cloud_adapter::adapter_for(cloudflare.path())
        .unwrap()
        .expect("wrangler.toml should detect Cloudflare");
    validate_install_owner(&cloudflare_adapter, cloudflare.path())
        .expect_err("Cloudflare must not skip the generic ownership gate");

    let cdk = tempdir().unwrap();
    std::fs::write(
        cdk.path().join("mgc.toml"),
        "name = \"demo\"\necosystem = \"cloud\"\n[cloud]\ntype = \"cdk\"\n",
    )
    .unwrap();
    std::fs::write(
        cdk.path().join("package.json"),
        r#"{"name":"demo","dependencies":{"aws-cdk-lib":"^2.0.0"}}"#,
    )
    .unwrap();
    let cdk_adapter = mgc_cloud_adapter::adapter_for(cdk.path())
        .unwrap()
        .expect("CDK project should detect a cloud adapter");
    validate_install_owner(&cdk_adapter, cdk.path())
        .expect("CDK with package.json should route through the native Web engine");
    validate_install_owner_for_op(
        &cdk_adapter,
        cdk.path(),
        crate::commands::dep_gate::DepOp::OfflineReinstall,
    )
    .expect("CDK embeds the Web engine's cache-only offline installer");
}
