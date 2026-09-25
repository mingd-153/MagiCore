#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_lockfile::{EcosystemTag, Lockfile, Package, parser, writer};

use super::{write_canonical_lock, write_flutter_package_config};
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
    let package_root = cache.path().join("hosted/pub.dev/http-1.2.0");
    std::fs::create_dir_all(package_root.join("lib")).unwrap();
    std::fs::write(
        package_root.join("pubspec.yaml"),
        "name: http\nenvironment:\n  sdk: '>=3.2.0 <4.0.0'\n",
    )
    .unwrap();
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
}

#[test]
fn flutter_package_config_rejects_archive_identity_mismatch() {
    let project = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
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

#[cfg(unix)]
#[test]
fn flutter_package_config_refuses_symlinked_dart_tool_directory() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), project.path().join(".dart_tool")).unwrap();

    let error =
        write_flutter_package_config(&ResolvedGraph::empty(), project.path(), outside.path())
            .unwrap_err();
    assert!(error.to_string().contains("linked"));
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}
