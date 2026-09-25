//! Canonical lock publication tests — test atomic updates and ecosystem pruning.
//! Kiểm thử ghi lock canonical — bảo vệ cập nhật nguyên tử và dọn theo ecosystem.

use super::{
    complete_lock_packages_from_existing, native_python_path_entries, python_site_dirname,
    read_existing_lock, validate_lock_coverage, validate_python_importable_graph,
    write_canonical_lock,
};
use mgc_lockfile::{EcosystemTag, Lockfile, Package, parser, writer};
use mgc_types::{PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version};

fn package(name: &str, version: &str, ecosystem: EcosystemTag) -> Package {
    Package {
        name: name.to_string(),
        version: version.to_string(),
        ecosystem,
        ..Package::default()
    }
}

fn resolved(name: &str, version: &str) -> ResolvedPackage {
    ResolvedPackage {
        id: PackageId::new(
            PackageName::new(name).unwrap(),
            Version::parse(version).unwrap(),
        ),
        integrity: "sha256-test".to_string(),
        tarball_url: "https://example.invalid/package.tgz".to_string(),
        deps: vec![],
        peer_deps: vec![],
        direct: true,
        dev: false,
    }
}

#[test]
fn install_refuses_graph_without_matching_lock_entry() {
    let graph = ResolvedGraph {
        packages: vec![resolved("python-dep", "1.2.3")],
    };
    let err = validate_lock_coverage(&graph, EcosystemTag::Python, &[]).unwrap_err();
    assert!(err.to_string().contains("no matching integrity/lock entry"));
}

#[test]
fn lib_writer_refuses_signed_lock_and_preserves_signature_pair() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    let mut signed = mgc_lockfile::Lockfile::new();
    signed
        .packages
        .push(package("old-lib-dep", "1.0.0", EcosystemTag::Python));
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    mgc_lockfile::sign_and_write_lockfile(&mut signed, &path, &key).unwrap();
    let lock_before = std::fs::read(&path).unwrap();
    let signature_path = path.with_extension("lock.sig");
    let signature_before = std::fs::read(&signature_path).unwrap();

    let result = write_canonical_lock(
        dir.path(),
        EcosystemTag::Python,
        vec![package("new-lib-dep", "2.0.0", EcosystemTag::Python)],
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
fn lib_writer_leaves_an_unchanged_signed_lock_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(dir.path(), "lib").unwrap();
    let path = dir.path().join("mgc.lock");
    let mut existing = mgc_lockfile::Lockfile::new();
    let mut existing_package = package("stable-lib-dep", "1.0.0", EcosystemTag::Python);
    existing_package.owner_core = Some("lib".to_string());
    existing.packages.push(existing_package);
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    mgc_lockfile::sign_and_write_lockfile(&mut existing, &path, &key).unwrap();
    let lock_before = std::fs::read(&path).unwrap();
    let signature_path = path.with_extension("lock.sig");
    let signature_before = std::fs::read(&signature_path).unwrap();
    let mut next_package = package("stable-lib-dep", "1.0.0", EcosystemTag::Python);
    next_package.owner_core = Some("lib".to_string());

    write_canonical_lock(dir.path(), EcosystemTag::Python, vec![next_package]).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), lock_before);
    assert_eq!(std::fs::read(&signature_path).unwrap(), signature_before);
    assert_eq!(
        mgc_lockfile::verify_lockfile(&path).unwrap(),
        mgc_lockfile::VerificationStatus::Valid
    );
}

#[test]
fn install_rejects_lock_entries_from_another_ecosystem() {
    let graph = ResolvedGraph::empty();
    let err = validate_lock_coverage(
        &graph,
        EcosystemTag::Python,
        &[package("rust-dep", "1.0.0", EcosystemTag::Rust)],
    )
    .unwrap_err();
    assert!(err.to_string().contains("outside active ecosystem"));
}

#[test]
fn python_install_rejects_unimportable_artifacts_during_preflight() {
    let mut compiled = resolved("numpy", "2.0.0");
    compiled.tarball_url =
        "https://files.pythonhosted.org/numpy-2.0.0-cp312-cp312-macosx_14_0_arm64.whl".to_string();
    let error = validate_python_importable_graph(&ResolvedGraph {
        packages: vec![compiled],
    })
    .unwrap_err();
    assert!(error.to_string().contains("does not yet support"));

    let mut source = resolved("example", "1.0.0");
    source.tarball_url = "https://files.pythonhosted.org/example-1.0.0.tar.gz".to_string();
    assert!(
        validate_python_importable_graph(&ResolvedGraph {
            packages: vec![source],
        })
        .is_err()
    );

    let mut pure = resolved("six", "1.17.0");
    pure.tarball_url = "https://files.pythonhosted.org/six-1.17.0-py3-none-any.whl".to_string();
    assert!(
        validate_python_importable_graph(&ResolvedGraph {
            packages: vec![pure],
        })
        .is_ok()
    );
}

#[test]
fn delta_install_completes_its_lock_from_existing_exact_pins() {
    let graph = ResolvedGraph {
        packages: vec![
            resolved("existing-dep", "1.0.0"),
            resolved("new-dep", "2.0.0"),
        ],
    };
    let existing = Lockfile {
        packages: vec![package("existing-dep", "1.0.0", EcosystemTag::Python)],
        ..Lockfile::new()
    };
    let mut delta = vec![package("new-dep", "2.0.0", EcosystemTag::Python)];

    complete_lock_packages_from_existing(&graph, EcosystemTag::Python, &mut delta, &existing);

    validate_lock_coverage(&graph, EcosystemTag::Python, &delta).unwrap();
    assert_eq!(delta.len(), 2);
}

#[test]
fn python_lock_update_preserves_packages_owned_by_another_core() {
    let root = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(root.path(), "lib").unwrap();
    let mut lock = Lockfile::new();
    lock.packages = vec![
        package("ai-only", "1.0.0", EcosystemTag::Python),
        package("lib-old", "1.0.0", EcosystemTag::Python),
    ];
    let mut serialized = writer::serialize_lockfile(&lock).unwrap();
    // Simulate lock entries written by the ownership-aware format. Keeping
    // this raw-field fixture lets the regression test fail against older
    // readers that silently ignore owner identity.
    serialized = serialized.replace(
        "name = \"ai-only\"",
        "name = \"ai-only\"\nowner_core = \"ai\"",
    );
    serialized = serialized.replace(
        "name = \"lib-old\"",
        "name = \"lib-old\"\nowner_core = \"lib\"",
    );
    std::fs::write(root.path().join("mgc.lock"), serialized).unwrap();

    write_canonical_lock(
        root.path(),
        EcosystemTag::Python,
        vec![package("lib-new", "2.0.0", EcosystemTag::Python)],
    )
    .unwrap();

    let updated = read_existing_lock(root.path()).unwrap();
    assert!(updated.packages.iter().any(|entry| entry.name == "ai-only"));
    assert!(!updated.packages.iter().any(|entry| entry.name == "lib-old"));
    assert!(updated.packages.iter().any(|entry| entry.name == "lib-new"));
    assert_eq!(
        updated
            .packages
            .iter()
            .find(|entry| entry.name == "lib-new")
            .and_then(|entry| entry.owner_core.as_deref()),
        Some("lib")
    );
}

#[test]
fn install_refuses_malformed_existing_lock_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    let original = b"malformed lock";
    std::fs::write(&path, original).unwrap();
    assert!(read_existing_lock(dir.path()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn python_runtime_paths_fail_closed_on_malformed_lock_and_ignore_absent_lock() {
    let dir = tempfile::tempdir().unwrap();
    assert!(native_python_path_entries(dir.path()).unwrap().is_empty());

    let path = dir.path().join("mgc.lock");
    std::fs::write(&path, "not valid lock TOML [[[").unwrap();
    assert!(
        native_python_path_entries(dir.path())
            .unwrap_err()
            .to_string()
            .contains("invalid mgc.lock for Python runtime")
    );
}

#[test]
fn python_site_path_rejects_untrusted_lock_traversal_segments() {
    assert!(python_site_dirname("six", "1.17.0").is_ok());
    for (name, version) in [
        ("../../outside", "1.0.0"),
        ("pkg", "1.0.0-../../outside"),
        ("pkg", r"1.0.0-..\outside"),
    ] {
        assert!(
            python_site_dirname(name, version).is_err(),
            "must reject {name}@{version}"
        );
    }
}

#[test]
fn canonical_lock_replaces_one_ecosystem_and_preserves_others() {
    let dir = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(dir.path(), "lib").unwrap();
    let mut old = Lockfile::new();
    old.packages
        .push(package("old-python", "1.0.0", EcosystemTag::Python));
    old.packages
        .push(package("retained-web", "2.0.0", EcosystemTag::Web));
    std::fs::write(
        dir.path().join("mgc.lock"),
        writer::serialize_lockfile(&old).unwrap(),
    )
    .unwrap();

    write_canonical_lock(
        dir.path(),
        EcosystemTag::Python,
        vec![package("new-python", "3.0.0", EcosystemTag::Python)],
    )
    .unwrap();

    let updated = parser::load_lockfile(&dir.path().join("mgc.lock")).unwrap();
    assert_eq!(updated.packages.len(), 2);
    assert!(updated.packages.iter().any(|p| p.name == "new-python"));
    assert!(updated.packages.iter().any(|p| p.name == "retained-web"));
    assert!(!updated.packages.iter().any(|p| p.name == "old-python"));
    assert!(
        updated
            .metadata
            .generated_at
            .parse::<chrono::DateTime<chrono::Utc>>()
            .is_ok()
    );
}

#[test]
fn empty_native_graph_removes_stale_entries_for_its_ecosystem() {
    let dir = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(dir.path(), "lib").unwrap();
    let mut old = Lockfile::new();
    old.packages
        .push(package("stale-python", "1.0.0", EcosystemTag::Python));
    old.packages
        .push(package("retained-web", "2.0.0", EcosystemTag::Web));
    std::fs::write(
        dir.path().join("mgc.lock"),
        writer::serialize_lockfile(&old).unwrap(),
    )
    .unwrap();

    write_canonical_lock(dir.path(), EcosystemTag::Python, vec![]).unwrap();

    let updated = parser::load_lockfile(&dir.path().join("mgc.lock")).unwrap();
    assert_eq!(updated.packages.len(), 1);
    assert_eq!(updated.packages[0].name, "retained-web");
}

#[test]
fn invalid_existing_lock_is_preserved_and_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.lock");
    let original = b"not = [valid TOML";
    std::fs::write(&path, original).unwrap();

    let result = write_canonical_lock(
        dir.path(),
        EcosystemTag::Python,
        vec![package("python-dep", "1.0.0", EcosystemTag::Python)],
    );

    assert!(result.is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn atomic_lock_publication_replaces_existing_file() {
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
