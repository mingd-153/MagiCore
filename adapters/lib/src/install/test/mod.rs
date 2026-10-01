//! Canonical lock publication tests — test atomic updates and ecosystem pruning.
//! Kiểm thử ghi lock canonical — bảo vệ cập nhật nguyên tử và dọn theo ecosystem.

use super::{
    complete_lock_packages_from_existing, native_python_path_entries,
    native_python_path_entries_from_store, python_site_dirname, read_cached_python_artifact,
    read_existing_lock, run_install, select_python_packages_for_owner, validate_lock_coverage,
    validate_python_importable_graph, write_canonical_lock, write_canonical_lock_with_roots,
};
use mgc_lockfile::{EcosystemTag, Lockfile, Package, parser, writer};
use mgc_types::adapter::InstallOptions;
use mgc_types::{PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version};
use std::io::{Read, Write};

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
fn python_artifact_cache_reuses_only_digest_verified_regular_files() {
    use sha2::Digest;

    let cache = tempfile::tempdir().unwrap();
    let bytes = b"wheel bytes";
    let expected = hex::encode(sha2::Sha256::digest(bytes));
    let artifact = cache.path().join("artifact.whl");

    assert_eq!(
        read_cached_python_artifact(&artifact, &expected).unwrap(),
        None
    );
    std::fs::write(&artifact, bytes).unwrap();
    assert_eq!(
        read_cached_python_artifact(&artifact, &expected).unwrap(),
        Some(bytes.to_vec())
    );
    std::fs::write(&artifact, b"tampered wheel").unwrap();
    let error = read_cached_python_artifact(&artifact, &expected).unwrap_err();
    assert!(error.to_string().contains("integrity mismatch"));
}

#[cfg(unix)]
#[test]
fn python_artifact_cache_refuses_symlink_without_reading_target() {
    use sha2::Digest;
    use std::os::unix::fs::symlink;

    let cache = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let bytes = b"outside wheel bytes";
    let expected = hex::encode(sha2::Sha256::digest(bytes));
    let target = outside.path().join("artifact.whl");
    let link = cache.path().join("artifact.whl");
    std::fs::write(&target, bytes).unwrap();
    symlink(&target, &link).unwrap();

    let error = read_cached_python_artifact(&link, &expected).unwrap_err();
    assert!(error.to_string().contains("regular file") || error.to_string().contains("open"));
    assert_eq!(std::fs::read(target).unwrap(), bytes);
}

fn python_wheel(name: &str, version: &str, module: &str, module_bytes: &[u8]) -> Vec<u8> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let dist_info = format!("{name}-{version}.dist-info");
    let metadata_path = format!("{dist_info}/METADATA");
    let record_path = format!("{dist_info}/RECORD");
    let metadata = format!("Name: {name}\nVersion: {version}\n");
    let files = [
        (module, module_bytes),
        (metadata_path.as_str(), metadata.as_bytes()),
    ];
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut record = String::new();
    for (path, bytes) in files {
        record.push_str(&format!(
            "{path},sha256={},{}\n",
            b64.encode(Sha256::digest(bytes)),
            bytes.len()
        ));
    }
    record.push_str(&format!("{record_path},,\n"));
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in files {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.start_file(record_path, options).unwrap();
    writer.write_all(record.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn install_refuses_graph_without_matching_lock_entry() {
    let graph = ResolvedGraph {
        packages: vec![resolved("python-dep", "1.2.3")],
    };
    let err = validate_lock_coverage(&graph, EcosystemTag::Python, &[]).unwrap_err();
    assert!(err.to_string().contains("no matching integrity/lock entry"));
}

#[tokio::test]
async fn native_lib_offline_install_fails_before_store_or_lock_mutation() {
    let project = tempfile::tempdir().unwrap();
    let error = run_install(
        crate::language::LibLanguage::Rust,
        None,
        &ResolvedGraph::empty(),
        project.path(),
        InstallOptions {
            offline: true,
            ..InstallOptions::default()
        },
        None,
        Vec::new(),
    )
    .await
    .expect_err("native lib lanes do not implement cache-only reinstall yet");

    assert!(
        error.to_string().contains("offline install is unsupported"),
        "unexpected failure did not prove the offline contract: {error:#}"
    );
    assert!(
        !project.path().join("mgc.lock").exists(),
        "unsupported offline install must not publish a lock"
    );
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
fn lib_lock_writer_records_direct_roots_without_erasing_sibling_ecosystems() {
    let dir = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(dir.path(), "lib").unwrap();
    let mut lock = Lockfile::new();
    let mut old_python = package("existing-python", "2.0.0", EcosystemTag::Python);
    old_python.owner_core = Some("lib".to_string());
    lock.packages.push(old_python);
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec![mgc_lockfile::format_root_pin(
            EcosystemTag::Python,
            "existing-python@2.0.0",
        )],
    );
    std::fs::write(
        dir.path().join("mgc.lock"),
        writer::serialize_lockfile(&lock).unwrap(),
    )
    .unwrap();

    write_canonical_lock_with_roots(
        dir.path(),
        EcosystemTag::Rust,
        vec![package("serde", "1.0.0", EcosystemTag::Rust)],
        vec!["serde@1.0.0".to_string()],
    )
    .unwrap();

    let updated = parser::load_lockfile(&dir.path().join("mgc.lock")).unwrap();
    assert_eq!(
        updated.root_dependencies_by_owner["lib"],
        vec![
            mgc_lockfile::format_root_pin(EcosystemTag::Python, "existing-python@2.0.0"),
            mgc_lockfile::format_root_pin(EcosystemTag::Rust, "serde@1.0.0"),
        ]
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

#[cfg(unix)]
#[test]
fn existing_lock_reader_refuses_symlinked_lockfile() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let target = external.path().join("mgc.lock");
    std::fs::write(&target, "version = \"3\"\n").unwrap();
    symlink(&target, project.path().join("mgc.lock")).unwrap();

    let error = read_existing_lock(project.path()).unwrap_err();

    assert!(error.to_string().to_lowercase().contains("symlink"));
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "version = \"3\"\n"
    );
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
            .contains("cannot safely read mgc.lock for Python runtime")
    );
}

#[test]
fn python_runtime_rejects_declared_dependencies_without_complete_mgc_lock() {
    let project = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(project.path(), "ai").unwrap();
    std::fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nname = \"runtime-probe\"\nversion = \"0.1.0\"\ndependencies = [\"six>=2.0\"]\n",
    )
    .unwrap();

    let missing_lock = native_python_path_entries(project.path()).unwrap_err();
    assert!(missing_lock.to_string().contains("mgc.lock"));

    let mut stale_lock = Lockfile::new();
    let mut stale_pin = package("six", "1.0.0", EcosystemTag::Python);
    stale_pin.owner_core = Some("ai".to_string());
    stale_lock.packages.push(stale_pin);
    std::fs::write(
        project.path().join("mgc.lock"),
        writer::serialize_lockfile(&stale_lock).unwrap(),
    )
    .unwrap();
    let stale_pin = native_python_path_entries(project.path()).unwrap_err();
    assert!(stale_pin.to_string().contains("requires '>=2.0'"));
    assert!(stale_pin.to_string().contains("pins '1.0.0'"));

    std::fs::write(
        project.path().join("mgc.lock"),
        writer::serialize_lockfile(&Lockfile::new()).unwrap(),
    )
    .unwrap();
    let incomplete_lock = native_python_path_entries(project.path()).unwrap_err();
    assert!(incomplete_lock.to_string().contains("six"));
}

#[test]
fn python_runtime_rejects_external_dependency_manifests_instead_of_ambient_fallback() {
    for manifest in ["requirements.txt", "uv.lock", "Pipfile", "setup.py"] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join(manifest),
            "# external dependency metadata\n",
        )
        .unwrap();

        let error = native_python_path_entries(project.path()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains(manifest.to_ascii_lowercase().as_str())
        );
        assert!(error.to_string().contains("ambient Python packages"));
    }
}

#[test]
fn python_runtime_verifies_cached_wheel_and_site_against_lock_before_exposing_path() {
    let project = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(project.path(), "ai").unwrap();
    std::fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nname = \"runtime-integrity-probe\"\nversion = \"0.1.0\"\ndependencies = [\"demo-pkg==1.2.3\"]\n",
    )
    .unwrap();

    let wheel_name = "demo_pkg-1.2.3-py3-none-any.whl";
    let wheel_url = format!("https://files.pythonhosted.org/packages/{wheel_name}");
    let module_path = "demo_pkg/__init__.py";
    let module_bytes = b"VALUE = 'trusted'\n";
    let wheel = python_wheel("demo_pkg", "1.2.3", module_path, module_bytes);
    let digest = mgc_resolver::protocols::sha256_hex(&wheel);
    let wheels = store.path().join("wheels");
    let artifact_path = wheels.join(
        mgc_resolver::protocols::PypiProtocol::artifact_cache_relpath(&wheel_url, &digest).unwrap(),
    );
    let site_dirname = mgc_resolver::protocols::PypiProtocol::importable_site_dirname_for_digest(
        "demo-pkg", "1.2.3", &digest,
    )
    .unwrap();
    let site_path = wheels.join("site").join(site_dirname);
    std::fs::create_dir_all(site_path.join("demo_pkg")).unwrap();
    std::fs::create_dir_all(site_path.join("demo_pkg-1.2.3.dist-info")).unwrap();
    std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
    std::fs::write(&artifact_path, &wheel).unwrap();
    std::fs::write(site_path.join(module_path), module_bytes).unwrap();
    let mut lock = Lockfile::new();
    let mut pinned = Package::new(
        "demo-pkg".into(),
        "1.2.3".into(),
        wheel_url,
        format!("sha256-{digest}"),
    );
    pinned.ecosystem = EcosystemTag::Python;
    pinned.owner_core = Some("ai".into());
    lock.add_package(pinned);
    std::fs::write(
        project.path().join("mgc.lock"),
        writer::serialize_lockfile(&lock).unwrap(),
    )
    .unwrap();
    std::fs::write(
        site_path.join("demo_pkg-1.2.3.dist-info/METADATA"),
        "Name: demo_pkg\nVersion: 1.2.3\n",
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&wheel)).unwrap();
    let record = archive.by_name("demo_pkg-1.2.3.dist-info/RECORD").unwrap();
    let mut record_bytes = Vec::new();
    record
        .take(16 * 1024)
        .read_to_end(&mut record_bytes)
        .unwrap();
    std::fs::write(
        site_path.join("demo_pkg-1.2.3.dist-info/RECORD"),
        record_bytes,
    )
    .unwrap();

    let paths = native_python_path_entries_from_store(project.path(), store.path()).unwrap();
    assert_eq!(paths.len(), 1);

    // Existing basename-keyed cache entries remain usable only through the
    // same lock digest and authenticated wheel RECORD checks.
    // (Cache cũ chỉ được đọc khi qua cùng kiểm tra digest và RECORD.)
    let legacy_site = wheels.join("site/demo-pkg-1.2.3");
    std::fs::rename(&artifact_path, wheels.join(wheel_name)).unwrap();
    std::fs::rename(&site_path, &legacy_site).unwrap();
    let legacy_paths = native_python_path_entries_from_store(project.path(), store.path()).unwrap();
    assert_eq!(legacy_paths, vec![legacy_site.clone()]);

    std::fs::write(
        legacy_paths[0].join(module_path),
        b"VALUE = 'tampered after install'\n",
    )
    .unwrap();
    let error = native_python_path_entries_from_store(project.path(), store.path()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("differs from authenticated wheel member")
    );
}

#[cfg(unix)]
#[test]
fn python_runtime_refuses_symlinked_lockfile() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let target = external.path().join("mgc.lock");
    std::fs::write(&target, "not a project lockfile\n").unwrap();
    symlink(&target, project.path().join("mgc.lock")).unwrap();

    let error = native_python_path_entries(project.path()).unwrap_err();

    assert!(error.to_string().contains("symlink"));
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "not a project lockfile\n"
    );
}

#[test]
fn existing_lock_reader_rejects_oversized_lockfile() {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("mgc.lock");
    std::fs::write(&path, vec![b'x'; 10 * 1024 * 1024 + 1]).unwrap();

    let error = read_existing_lock(project.path()).unwrap_err();

    assert!(error.to_string().contains("too large"));
}

#[test]
fn python_owner_selection_isolated_for_multi_core_lock_and_rejects_ambiguous_legacy_entries() {
    let mut ai = package("shared-name", "1.0.0", EcosystemTag::Python);
    ai.owner_core = Some("ai".to_string());
    let mut lib = package("shared-name", "2.0.0", EcosystemTag::Python);
    lib.owner_core = Some("lib".to_string());
    let lock = vec![ai, lib];

    let ai_selected = select_python_packages_for_owner(&lock, "ai").unwrap();
    let lib_selected = select_python_packages_for_owner(&lock, "lib").unwrap();
    assert_eq!(ai_selected.len(), 1);
    assert_eq!(ai_selected[0].version, "1.0.0");
    assert_eq!(lib_selected.len(), 1);
    assert_eq!(lib_selected[0].version, "2.0.0");

    let ambiguous = vec![
        lock[0].clone(),
        package("legacy", "1.0.0", EcosystemTag::Python),
    ];
    assert!(
        select_python_packages_for_owner(&ambiguous, "ai")
            .unwrap_err()
            .to_string()
            .contains("ambiguous core ownership")
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
