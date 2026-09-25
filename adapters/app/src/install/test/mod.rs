#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_lockfile::{EcosystemTag, Lockfile, Package, parser, writer};

use super::{
    write_canonical_lock, write_flutter_package_config, write_flutter_package_config_with_sdk_root,
};
use mgc_types::{PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version};

fn package(name: &str, version: &str, ecosystem: EcosystemTag) -> Package {
    Package {
        name: name.to_string(),
        version: version.to_string(),
        ecosystem,
        ..Default::default()
    }
}

#[test]
fn invalid_existing_lock_is_preserved_instead_of_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    let original = b"not = [valid TOML";
    std::fs::write(&path, original).unwrap();

    let result = write_canonical_lock(
        dir.path(),
        &[EcosystemTag::Dart],
        vec![package("flutter-dep", "1.0.0", EcosystemTag::Dart)],
    );

    assert!(result.is_err(), "malformed lock must fail closed");
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn app_writer_refuses_signed_lock_and_preserves_signature_pair() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    let mut signed = Lockfile::new();
    signed
        .packages
        .push(package("old-app-dep", "1.0.0", EcosystemTag::Dart));
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    mgc_lockfile::sign_and_write_lockfile(&mut signed, &path, &key).unwrap();
    let lock_before = std::fs::read(&path).unwrap();
    let signature_path = path.with_extension("lock.sig");
    let signature_before = std::fs::read(&signature_path).unwrap();

    let result = write_canonical_lock(
        dir.path(),
        &[EcosystemTag::Dart],
        vec![package("new-app-dep", "2.0.0", EcosystemTag::Dart)],
    );

    assert!(result.unwrap_err().to_string().contains("signed mgc.lock"));
    assert_eq!(std::fs::read(&path).unwrap(), lock_before);
    assert_eq!(std::fs::read(&signature_path).unwrap(), signature_before);
    assert_eq!(
        mgc_lockfile::verify_lockfile(&path).unwrap(),
        mgc_lockfile::VerificationStatus::Valid
    );
}

#[test]
fn app_writer_leaves_an_unchanged_signed_lock_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(dir.path(), "app").unwrap();
    let path = dir.path().join("mgc.lock");
    let mut existing = Lockfile::new();
    let mut existing_package = package("stable-app-dep", "1.0.0", EcosystemTag::Dart);
    existing_package.owner_core = Some("app".to_string());
    existing.packages.push(existing_package);
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    mgc_lockfile::sign_and_write_lockfile(&mut existing, &path, &key).unwrap();
    let lock_before = std::fs::read(&path).unwrap();
    let signature_path = path.with_extension("lock.sig");
    let signature_before = std::fs::read(&signature_path).unwrap();
    let mut next_package = package("stable-app-dep", "1.0.0", EcosystemTag::Dart);
    next_package.owner_core = Some("app".to_string());

    write_canonical_lock(dir.path(), &[EcosystemTag::Dart], vec![next_package]).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), lock_before);
    assert_eq!(std::fs::read(&signature_path).unwrap(), signature_before);
    assert_eq!(
        mgc_lockfile::verify_lockfile(&path).unwrap(),
        mgc_lockfile::VerificationStatus::Valid
    );
}

#[test]
fn canonical_lock_update_preserves_same_name_and_version_from_other_ecosystem() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = Lockfile::new();
    old.packages
        .push(package("shared-name", "1.0.0", EcosystemTag::Web));
    old.packages
        .push(package("stale-flutter", "1.0.0", EcosystemTag::Dart));
    std::fs::write(
        dir.path().join("mgc.lock"),
        writer::serialize_lockfile(&old).unwrap(),
    )
    .unwrap();

    write_canonical_lock(
        dir.path(),
        &[EcosystemTag::Dart],
        vec![package("shared-name", "1.0.0", EcosystemTag::Dart)],
    )
    .unwrap();

    let updated = parser::load_lockfile(&dir.path().join("mgc.lock")).unwrap();
    assert!(
        updated
            .packages
            .iter()
            .any(|p| { p.name == "shared-name" && p.ecosystem == EcosystemTag::Web })
    );
    assert!(
        updated
            .packages
            .iter()
            .any(|p| { p.name == "shared-name" && p.ecosystem == EcosystemTag::Dart })
    );
    assert!(!updated.packages.iter().any(|p| p.name == "stale-flutter"));
}

#[test]
fn app_lock_update_preserves_same_ecosystem_entries_owned_by_another_core() {
    let dir = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(dir.path(), "app").unwrap();
    let mut old = Lockfile::new();
    old.packages.push(Package {
        owner_core: Some("ai".to_string()),
        name: "shared-python-package".to_string(),
        version: "2.0.0".to_string(),
        ecosystem: EcosystemTag::Dart,
        ..Default::default()
    });
    old.packages.push(Package {
        owner_core: Some("app".to_string()),
        name: "stale-app-package".to_string(),
        version: "1.0.0".to_string(),
        ecosystem: EcosystemTag::Dart,
        ..Default::default()
    });
    std::fs::write(
        dir.path().join("mgc.lock"),
        writer::serialize_lockfile(&old).unwrap(),
    )
    .unwrap();

    write_canonical_lock(
        dir.path(),
        &[EcosystemTag::Dart],
        vec![Package {
            name: "new-app-package".to_string(),
            version: "3.0.0".to_string(),
            ecosystem: EcosystemTag::Dart,
            ..Default::default()
        }],
    )
    .unwrap();

    let updated = parser::load_lockfile(&dir.path().join("mgc.lock")).unwrap();
    assert!(updated.packages.iter().any(|package| {
        package.owner_core.as_deref() == Some("ai")
            && package.name == "shared-python-package"
            && package.version == "2.0.0"
    }));
    assert!(
        !updated
            .packages
            .iter()
            .any(|package| package.name == "stale-app-package")
    );
    assert!(updated.packages.iter().any(|package| {
        package.owner_core.as_deref() == Some("app") && package.name == "new-app-package"
    }));
}

#[test]
fn empty_app_lock_scope_removes_stale_entries_without_touching_other_ecosystems() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = Lockfile::new();
    old.packages
        .push(package("stale-flutter", "1.0.0", EcosystemTag::Dart));
    old.packages
        .push(package("retained-swift", "2.0.0", EcosystemTag::Swift));
    std::fs::write(
        dir.path().join("mgc.lock"),
        writer::serialize_lockfile(&old).unwrap(),
    )
    .unwrap();

    write_canonical_lock(dir.path(), &[EcosystemTag::Dart], vec![]).unwrap();

    let updated = parser::load_lockfile(&dir.path().join("mgc.lock")).unwrap();
    assert_eq!(updated.packages.len(), 1);
    assert_eq!(updated.packages[0].name, "retained-swift");
}

#[test]
fn lock_writer_rejects_entries_outside_the_declared_ecosystem_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    let result = write_canonical_lock(
        dir.path(),
        &[EcosystemTag::Dart],
        vec![package("web-dep", "1.0.0", EcosystemTag::Web)],
    );
    assert!(result.is_err());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn lock_writer_refuses_symlink_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.lock");
    let link = dir.path().join("mgc.lock");
    let original = b"untrusted target bytes";
    std::fs::write(&target, original).unwrap();
    symlink(&target, &link).unwrap();

    let result = write_canonical_lock(
        dir.path(),
        &[EcosystemTag::Dart],
        vec![package("flutter-dep", "1.0.0", EcosystemTag::Dart)],
    );

    assert!(result.is_err());
    assert_eq!(std::fs::read(target).unwrap(), original);
    assert!(
        std::fs::symlink_metadata(link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn atomic_lock_publish_replaces_existing_file_without_leaking_temp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    std::fs::write(
        &path,
        b"version = \"3\"\n[metadata]\ngenerated_at = \"1970-01-01T00:00:00Z\"\ngenerator = \"fixture\"\nlockfile_hash = \"\"\n",
    )
    .unwrap();

    let replacement = b"version = \"3\"\n[metadata]\ngenerated_at = \"1970-01-01T00:00:01Z\"\ngenerator = \"fixture\"\nlockfile_hash = \"\"\n";
    super::atomic_write_canonical_lock(&path, replacement).unwrap();

    assert_eq!(std::fs::read(path).unwrap(), replacement);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn flutter_install_writes_package_config_for_verified_materialized_graph() {
    let project = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\nversion: 1.0.0\ndependencies:\n  http: ^1.2.0\ndev_dependencies:\n  meta: ^1.0.0\n",
    )
    .unwrap();
    let package_root = cache.path().join("hosted/pub.dev/http-1.2.0");
    std::fs::create_dir_all(package_root.join("lib")).unwrap();
    std::fs::write(
        package_root.join("pubspec.yaml"),
        "name: http\nenvironment:\n  sdk: '>=3.2.0 <4.0.0'\n",
    )
    .unwrap();
    let meta_root = cache.path().join("hosted/pub.dev/meta-1.0.0");
    std::fs::create_dir_all(meta_root.join("lib")).unwrap();
    std::fs::write(meta_root.join("pubspec.yaml"), "name: meta\n").unwrap();
    let graph = ResolvedGraph {
        packages: vec![
            ResolvedPackage {
                id: PackageId::new(
                    PackageName::new("http").unwrap(),
                    Version::parse("1.2.0").unwrap(),
                ),
                integrity: "sha256-fixture".to_string(),
                tarball_url: "https://pub.dev/http.tar.gz".to_string(),
                deps: vec![PackageId::new(
                    PackageName::new("meta").unwrap(),
                    Version::parse("1.0.0").unwrap(),
                )],
                peer_deps: vec![],
                direct: true,
                dev: false,
            },
            ResolvedPackage {
                id: PackageId::new(
                    PackageName::new("meta").unwrap(),
                    Version::parse("1.0.0").unwrap(),
                ),
                integrity: "sha256-meta-fixture".to_string(),
                tarball_url: "https://pub.dev/meta.tar.gz".to_string(),
                deps: vec![],
                peer_deps: vec![],
                direct: false,
                dev: true,
            },
        ],
    };

    write_flutter_package_config(&graph, project.path(), cache.path()).unwrap();

    let config: serde_json::Value = serde_json::from_slice(
        &std::fs::read(project.path().join(".dart_tool/package_config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["configVersion"], 2);
    assert_eq!(config["generator"], "MagiCore");
    assert_eq!(config["packages"][0]["name"], "http");
    assert_eq!(config["packages"][0]["packageUri"], "lib/");
    assert_eq!(config["packages"][0]["languageVersion"], "3.2");
    assert!(
        config["packages"][0]["rootUri"]
            .as_str()
            .unwrap()
            .starts_with("file://")
    );

    let package_graph: serde_json::Value = serde_json::from_slice(
        &std::fs::read(project.path().join(".dart_tool/package_graph.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(package_graph["configVersion"], 1);
    assert_eq!(package_graph["roots"], serde_json::json!(["sample"]));
    assert_eq!(
        package_graph["packages"],
        serde_json::json!([
            {
                "name": "sample",
                "version": "1.0.0",
                "dependencies": ["http"],
                "devDependencies": ["meta"]
            },
            {"name": "http", "version": "1.2.0", "dependencies": ["meta"]},
            {"name": "meta", "version": "1.0.0", "dependencies": []}
        ])
    );
}

#[test]
fn flutter_package_config_rejects_archive_identity_mismatch() {
    let project = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("pubspec.yaml"), "name: sample\n").unwrap();
    let package_root = cache.path().join("hosted/pub.dev/http-1.2.0");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(package_root.join("pubspec.yaml"), "name: different\n").unwrap();
    let graph = ResolvedGraph {
        packages: vec![ResolvedPackage {
            id: PackageId::new(
                PackageName::new("http").unwrap(),
                Version::parse("1.2.0").unwrap(),
            ),
            integrity: "sha256-fixture".to_string(),
            tarball_url: "https://pub.dev/http.tar.gz".to_string(),
            deps: vec![],
            peer_deps: vec![],
            direct: true,
            dev: false,
        }],
    };

    let error = write_flutter_package_config(&graph, project.path(), cache.path()).unwrap_err();
    assert!(error.to_string().contains("identity mismatch"));
    assert!(
        !project
            .path()
            .join(".dart_tool/package_config.json")
            .exists()
    );
}

#[test]
fn flutter_package_config_includes_only_declared_sdk_packages() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\ndev_dependencies:\n  flutter_test:\n    sdk: flutter\n",
    )
    .unwrap();
    for (name, version) in [
        ("flutter", ">=3.4.0-0 <4.0.0"),
        ("flutter_test", ">=3.4.0-0 <4.0.0"),
    ] {
        let package = sdk.path().join("packages").join(name);
        std::fs::create_dir_all(package.join("lib")).unwrap();
        std::fs::write(
            package.join("pubspec.yaml"),
            format!("name: {name}\nenvironment:\n  sdk: '{version}'\n"),
        )
        .unwrap();
    }
    write_flutter_package_config_with_sdk_root(
        &ResolvedGraph::empty(),
        project.path(),
        cache.path(),
        Some(sdk.path()),
    )
    .unwrap();

    let config: serde_json::Value = serde_json::from_slice(
        &std::fs::read(project.path().join(".dart_tool/package_config.json")).unwrap(),
    )
    .unwrap();
    let packages = config["packages"].as_array().unwrap();
    let names: Vec<_> = packages
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert_eq!(names, ["flutter", "flutter_test"]);
    assert!(
        packages
            .iter()
            .all(|entry| entry["rootUri"].as_str().unwrap().starts_with("file://"))
    );
}

#[test]
fn flutter_sdk_package_dependencies_enter_native_pub_graph() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\n  meta: ^1.17.0\ndev_dependencies:\n  flutter_test:\n    sdk: flutter\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("packages/flutter/lib")).unwrap();
    std::fs::write(
        sdk.path().join("packages/flutter/pubspec.yaml"),
        "name: flutter\ndependencies:\n  collection: ^1.18.0\n  meta: ^1.18.0\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("packages/flutter_test/lib")).unwrap();
    std::fs::write(
        sdk.path().join("packages/flutter_test/pubspec.yaml"),
        "name: flutter_test\ndependencies:\n  flutter:\n    sdk: flutter\n  sky_engine:\n    sdk: flutter\n  matcher: ^0.12.16\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("bin/cache/pkg/sky_engine/lib")).unwrap();
    std::fs::write(
        sdk.path().join("bin/cache/pkg/sky_engine/pubspec.yaml"),
        "name: sky_engine\n",
    )
    .unwrap();

    let base =
        crate::manifest::parse_manifest(crate::AppLanguage::Flutter, project.path()).unwrap();
    let expanded = crate::manifest::flutter::expand_flutter_sdk_dependencies_with_root(
        project.path(),
        &base,
        sdk.path(),
    )
    .unwrap();
    assert!(expanded.find_dep("collection").is_some());
    assert!(expanded.find_dep("matcher").is_some());
    let meta_range = &expanded.find_dep("meta").unwrap().range;
    assert!(meta_range.matches(&Version::parse("1.18.0").unwrap()));
    assert!(!meta_range.matches(&Version::parse("1.17.0").unwrap()));
    assert!(
        expanded
            .dependencies
            .iter()
            .any(|dependency| dependency.name.as_str() == "collection")
    );
    assert!(
        expanded
            .dev_dependencies
            .iter()
            .any(|dependency| dependency.name.as_str() == "matcher")
    );
    assert!(expanded.find_dep("sky_engine").is_none());

    let names =
        crate::manifest::flutter::flutter_sdk_package_names(project.path(), sdk.path()).unwrap();
    assert_eq!(names, ["flutter", "flutter_test", "sky_engine"]);
}

#[test]
fn flutter_sdk_runtime_dependency_promotes_project_optional_package_to_runtime() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\ndev_dependencies:\n  meta: ^1.17.0\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("packages/flutter/lib")).unwrap();
    std::fs::write(
        sdk.path().join("packages/flutter/pubspec.yaml"),
        "name: flutter\ndependencies:\n  meta: ^1.18.0\n",
    )
    .unwrap();

    let base =
        crate::manifest::parse_manifest(crate::AppLanguage::Flutter, project.path()).unwrap();
    let expanded = crate::manifest::flutter::expand_flutter_sdk_dependencies_with_root(
        project.path(),
        &base,
        sdk.path(),
    )
    .unwrap();

    assert!(
        expanded
            .dependencies
            .iter()
            .any(|dependency| dependency.name.as_str() == "meta"),
        "a Flutter SDK runtime dependency must promote the same project dev dependency"
    );
    assert!(
        !expanded
            .dev_dependencies
            .iter()
            .any(|dependency| dependency.name.as_str() == "meta"),
        "meta must not remain dev-only when Flutter requires it at runtime"
    );
    assert!(
        expanded
            .find_dep("meta")
            .unwrap()
            .range
            .matches(&Version::parse("1.18.0").unwrap())
    );
}

#[test]
fn flutter_sdk_path_lookup_is_filesystem_only_and_honors_pathext() {
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("flutter.CMD");
    std::fs::write(&executable, "@echo off\r\n").unwrap();

    let found = crate::manifest::flutter::find_flutter_executable_in_path(
        dir.path().as_os_str(),
        Some(std::ffi::OsStr::new(".COM;.EXE;.CMD;.BAT")),
        true,
    );

    assert_eq!(found, Some(executable.canonicalize().unwrap()));
}

#[test]
fn flutter_sdk_path_lookup_does_not_accept_missing_or_directory_candidates() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("flutter")).unwrap();

    let found = crate::manifest::flutter::find_flutter_executable_in_path(
        dir.path().as_os_str(),
        None,
        false,
    );

    assert_eq!(found, None);
}

#[test]
fn flutter_sdk_package_malformed_dependency_metadata_fails_closed() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("packages/flutter/lib")).unwrap();
    std::fs::write(
        sdk.path().join("packages/flutter/pubspec.yaml"),
        "name: flutter\ndependencies: [not, a, mapping]\n",
    )
    .unwrap();

    let base =
        crate::manifest::parse_manifest(crate::AppLanguage::Flutter, project.path()).unwrap();
    let error = crate::manifest::flutter::expand_flutter_sdk_dependencies_with_root(
        project.path(),
        &base,
        sdk.path(),
    )
    .expect_err("malformed SDK dependency metadata must not become an empty graph");
    assert!(error.to_string().contains("dependencies must be a mapping"));
}

#[test]
fn flutter_sdk_disjunctive_constraints_fail_closed_when_intersection_is_needed() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\n  meta: '>=1.17.0 <2.0.0 || >=3.0.0 <4.0.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("packages/flutter/lib")).unwrap();
    std::fs::write(
        sdk.path().join("packages/flutter/pubspec.yaml"),
        "name: flutter\ndependencies:\n  meta: '>=1.18.0 <2.0.0'\n",
    )
    .unwrap();

    let base =
        crate::manifest::parse_manifest(crate::AppLanguage::Flutter, project.path()).unwrap();
    let error = crate::manifest::flutter::expand_flutter_sdk_dependencies_with_root(
        project.path(),
        &base,
        sdk.path(),
    )
    .expect_err("constraint intersection must not weaken an OR expression");
    assert!(error.to_string().contains("disjunctive constraints"));
}

#[test]
fn flutter_sdk_dependency_overrides_fail_closed() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    std::fs::create_dir_all(sdk.path().join("packages/flutter/lib")).unwrap();
    std::fs::write(
        sdk.path().join("packages/flutter/pubspec.yaml"),
        "name: flutter\ndependency_overrides:\n  meta: 1.0.0\n",
    )
    .unwrap();

    let base =
        crate::manifest::parse_manifest(crate::AppLanguage::Flutter, project.path()).unwrap();
    let error = crate::manifest::flutter::expand_flutter_sdk_dependencies_with_root(
        project.path(),
        &base,
        sdk.path(),
    )
    .expect_err("SDK overrides must not silently be ignored");
    assert!(error.to_string().contains("override/workspace semantics"));
}

#[test]
fn flutter_pubspec_writer_preserves_all_sdk_dependencies() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\ndev_dependencies:\n  flutter_test:\n    sdk: flutter\n  integration_test:\n    sdk: flutter\n",
    )
    .unwrap();
    let manifest =
        crate::manifest::parse_manifest(crate::AppLanguage::Flutter, project.path()).unwrap();

    crate::manifest::flutter::write_pubspec(project.path(), &manifest).unwrap();

    let rewritten: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(project.path().join("pubspec.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        rewritten["dependencies"]["flutter"]["sdk"].as_str(),
        Some("flutter")
    );
    for name in ["flutter_test", "integration_test"] {
        assert_eq!(
            rewritten["dev_dependencies"][name]["sdk"].as_str(),
            Some("flutter"),
            "writer dropped SDK test dependency {name}"
        );
    }
}

#[test]
fn flutter_package_config_fails_closed_when_declared_sdk_package_is_missing() {
    let project = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("pubspec.yaml"),
        "name: sample\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    let error = write_flutter_package_config_with_sdk_root(
        &ResolvedGraph::empty(),
        project.path(),
        cache.path(),
        Some(sdk.path()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("SDK package 'flutter'"));
    assert!(
        !project.path().join(".dart_tool").exists(),
        "validation error must not leave a partial package config directory"
    );
}

#[cfg(unix)]
#[test]
fn flutter_package_config_refuses_symlinked_dart_tool_directory() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("pubspec.yaml"), "name: sample\n").unwrap();
    symlink(outside.path(), project.path().join(".dart_tool")).unwrap();

    let error =
        write_flutter_package_config(&ResolvedGraph::empty(), project.path(), outside.path())
            .unwrap_err();
    assert!(error.to_string().contains("linked"));
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}
