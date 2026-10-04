use super::*;
use mgc_store::ContentStore;

#[test]
fn warm_extraction_imports_into_the_validated_canonical_root() {
    let shared = tempdir_real().unwrap();
    let project = tempdir_real().unwrap();
    let package_id = PackageId::new(
        PackageName::new("warm-root-probe").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    let integrity = seed_shared_tarball_with_files(
        shared.path(),
        &package_id,
        &[
            (
                "package/package.json",
                br#"{"name":"warm-root-probe","version":"1.0.0"}"#,
            ),
            ("package/index.js", b"module.exports = true;\n"),
        ],
    );
    let package = ResolvedPackage {
        id: package_id.clone(),
        integrity,
        tarball_url: String::new(),
        deps: vec![],
        peer_deps: vec![],
        direct: true,
        dev: false,
    };
    let shared_cache = SharedWebCache {
        root: shared.path().to_path_buf(),
    };
    let tarball = PackageCache::new(shared.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let marker = expected_extracted_package_marker_from_bytes(&package, &tarball).unwrap();
    let canonical_root = shared_cache.extracted_package_root(&package);
    std::fs::create_dir_all(&canonical_root).unwrap();
    std::fs::write(
        canonical_root.join("package.json"),
        br#"{"name":"warm-root-probe","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(canonical_root.join("index.js"), b"module.exports = true;\n").unwrap();
    write_extracted_package_marker(&canonical_root, &marker).unwrap();

    let layout = Layout::new(project.path().join(".magicore/cache/web"));
    let store = ContentStore::new(layout.cas_dir()).unwrap();
    let returned = ensure_extracted_package_root_with_marker(
        &layout,
        &store,
        Some(&shared_cache),
        &package,
        &marker,
        |_| panic!("the valid warm cache should not be re-extracted"),
        |extraction_root| {
            assert_eq!(
                extraction_root, canonical_root,
                "the validated canonical package tree should be the CAS import target"
            );
            let imported = mgc_fetcher::extract::extract_tarball_to_cas_and_verify_existing_root(
                std::io::Cursor::new(tarball.as_slice()),
                extraction_root,
                &store,
                None,
            )
            .unwrap();
            assert!(
                imported,
                "the package/ archive layout should take the warm path"
            );
            Ok(imported)
        },
    )
    .unwrap();

    assert_eq!(returned, canonical_root);
}

#[test]
fn warm_extraction_rejects_a_tampered_tree_with_a_matching_marker() {
    let shared = tempdir_real().unwrap();
    let tampered_source = tempdir_real().unwrap();
    let project = tempdir_real().unwrap();
    let package_id = PackageId::new(
        PackageName::new("warm-marker-probe").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    let integrity = seed_shared_tarball_with_files(
        shared.path(),
        &package_id,
        &[
            (
                "package/package.json",
                br#"{"name":"warm-marker-probe","version":"1.0.0"}"#,
            ),
            ("package/index.js", b"module.exports = 'trusted';\n"),
        ],
    );
    let package = ResolvedPackage {
        id: package_id.clone(),
        integrity,
        tarball_url: String::new(),
        deps: vec![],
        peer_deps: vec![],
        direct: true,
        dev: false,
    };
    let shared_cache = SharedWebCache {
        root: shared.path().to_path_buf(),
    };
    let trusted_tarball = PackageCache::new(shared.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let expected_marker =
        expected_extracted_package_marker_from_bytes(&package, &trusted_tarball).unwrap();

    seed_shared_tarball_with_files(
        tampered_source.path(),
        &package_id,
        &[
            (
                "package/package.json",
                br#"{"name":"warm-marker-probe","version":"1.0.0"}"#,
            ),
            ("package/index.js", b"module.exports = 'tampered';\n"),
        ],
    );
    let tampered_tarball = PackageCache::new(tampered_source.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let tampered_marker =
        expected_extracted_package_marker_from_bytes(&package, &tampered_tarball).unwrap();
    assert!(extracted_marker_matches_fast(
        &tampered_marker,
        &expected_marker
    ));

    let canonical_root = shared_cache.extracted_package_root(&package);
    std::fs::create_dir_all(&canonical_root).unwrap();
    std::fs::write(
        canonical_root.join("package.json"),
        br#"{"name":"warm-marker-probe","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        canonical_root.join("index.js"),
        b"module.exports = 'tampered';\n",
    )
    .unwrap();
    write_extracted_package_marker(&canonical_root, &tampered_marker).unwrap();
    assert!(extracted_content_matches(&canonical_root, &tampered_marker).unwrap());

    let layout = Layout::new(project.path().join(".magicore/cache/web"));
    let store = ContentStore::new(layout.cas_dir()).unwrap();
    let warm_imports = AtomicUsize::new(0);
    let error = ensure_extracted_package_root_with_marker(
        &layout,
        &store,
        Some(&shared_cache),
        &package,
        &expected_marker,
        |_| Err(MgError::Other("cold rebuild selected".into())),
        |_| {
            warm_imports.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("cold rebuild selected"));
    assert_eq!(warm_imports.load(Ordering::SeqCst), 0);
}

#[test]
fn verified_warm_archive_import_matches_cache_before_claiming_cas() {
    let cache = tempdir_real().unwrap();
    let project = tempdir_real().unwrap();
    let package_id = PackageId::new(
        PackageName::new("warm-archive-verify-probe").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    let integrity = seed_shared_tarball_with_files(
        cache.path(),
        &package_id,
        &[
            (
                "package/package.json",
                br#"{"name":"warm-archive-verify-probe","version":"1.0.0"}"#,
            ),
            ("package/index.js", b"module.exports = true;\n"),
        ],
    );
    let package = ResolvedPackage {
        id: package_id.clone(),
        integrity,
        tarball_url: String::new(),
        deps: vec![],
        peer_deps: vec![],
        direct: true,
        dev: false,
    };
    let tarball = PackageCache::new(cache.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let marker = expected_extracted_package_marker_from_bytes(&package, &tarball).unwrap();
    let package_root = cache.path().join("extracted");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(
        package_root.join("package.json"),
        br#"{"name":"warm-archive-verify-probe","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(package_root.join("index.js"), b"module.exports = true;\n").unwrap();
    write_extracted_package_marker(&package_root, &marker).unwrap();

    let layout = Layout::new(project.path().join(".magicore/cache/web"));
    let store = ContentStore::new(layout.cas_dir()).unwrap();
    let imported = import_and_verify_cached_root_from_tarball(
        std::io::Cursor::new(tarball.as_slice()),
        &package_root,
        &marker,
        &store,
        None,
    )
    .unwrap();

    assert!(imported, "the matching package/ cache should be reused");
    let index_blob = mgc_store::IntegrityHash::from_bytes(b"module.exports = true;\n", false);
    assert!(index_blob.cas_path(&layout.cas_dir()).is_file());
    assert_eq!(
        std::fs::read(package_root.join("index.js")).unwrap(),
        b"module.exports = true;\n",
        "warm reuse must not rewrite the canonical package tree"
    );
}

#[test]
fn ensure_extracted_package_root_from_bytes_uses_verified_warm_path_and_files_claims() {
    let shared = tempdir_real().unwrap();
    let project = tempdir_real().unwrap();
    let package_id = PackageId::new(
        PackageName::new("warm-route-probe").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    let integrity = seed_shared_tarball_with_files(
        shared.path(),
        &package_id,
        &[
            (
                "package/package.json",
                br#"{"name":"warm-route-probe","version":"1.0.0"}"#,
            ),
            ("package/index.js", b"module.exports = 'route';\n"),
        ],
    );
    let package = ResolvedPackage {
        id: package_id.clone(),
        integrity,
        tarball_url: String::new(),
        deps: vec![],
        peer_deps: vec![],
        direct: true,
        dev: false,
    };
    let tarball = PackageCache::new(shared.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let marker = expected_extracted_package_marker_from_bytes(&package, &tarball).unwrap();
    let shared_cache = SharedWebCache {
        root: shared.path().to_path_buf(),
    };
    let package_root = shared_cache.extracted_package_root(&package);
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(
        package_root.join("package.json"),
        br#"{"name":"warm-route-probe","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        package_root.join("index.js"),
        b"module.exports = 'route';\n",
    )
    .unwrap();
    write_extracted_package_marker(&package_root, &marker).unwrap();

    let layout = Layout::new(project.path().join(".magicore/cache/web"));
    let store = ContentStore::new(layout.cas_dir()).unwrap();
    let db = mgc_store::Database::open(&layout.db_path()).unwrap();
    let project_key = layout.root().to_string_lossy().into_owned();
    let generation = db.begin_cas_generation(&project_key).unwrap();
    let returned = ensure_extracted_package_root_from_bytes(
        &layout,
        &store,
        Some(&shared_cache),
        &package,
        &tarball,
        generation,
    )
    .unwrap();

    assert_eq!(returned, package_root);
    assert_eq!(
        std::fs::read(package_root.join("index.js")).unwrap(),
        b"module.exports = 'route';\n",
        "the warm path must reuse the canonical package tree"
    );
    let index_blob = mgc_store::IntegrityHash::from_bytes(b"module.exports = 'route';\n", false);
    assert!(
        db.list_cas_live_refs()
            .unwrap()
            .contains(&index_blob.as_hex().to_string())
    );
}

#[test]
fn verified_warm_archive_import_rejects_changed_or_extra_cache_files() {
    let cache = tempdir_real().unwrap();
    let package_id = PackageId::new(
        PackageName::new("warm-archive-reject-probe").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    let integrity = seed_shared_tarball_with_files(
        cache.path(),
        &package_id,
        &[
            (
                "package/package.json",
                br#"{"name":"warm-archive-reject-probe","version":"1.0.0"}"#,
            ),
            ("package/index.js", b"module.exports = 'trusted';\n"),
        ],
    );
    let package = ResolvedPackage {
        id: package_id.clone(),
        integrity,
        tarball_url: String::new(),
        deps: vec![],
        peer_deps: vec![],
        direct: true,
        dev: false,
    };
    let tarball = PackageCache::new(cache.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let marker = expected_extracted_package_marker_from_bytes(&package, &tarball).unwrap();
    let package_root = cache.path().join("extracted");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(
        package_root.join("package.json"),
        br#"{"name":"warm-archive-reject-probe","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        package_root.join("index.js"),
        b"module.exports = 'changed';\n",
    )
    .unwrap();
    write_extracted_package_marker(&package_root, &marker).unwrap();
    let store = ContentStore::new(cache.path().join("cas")).unwrap();

    let imported = import_and_verify_cached_root_from_tarball(
        std::io::Cursor::new(tarball.as_slice()),
        &package_root,
        &marker,
        &store,
        None,
    )
    .unwrap();

    assert!(!imported, "a changed cached file must miss the warm path");
    std::fs::write(
        package_root.join("index.js"),
        b"module.exports = 'trusted';\n",
    )
    .unwrap();
    std::fs::write(package_root.join("extra.js"), b"unexpected\n").unwrap();
    let imported_with_extra = import_and_verify_cached_root_from_tarball(
        std::io::Cursor::new(tarball.as_slice()),
        &package_root,
        &marker,
        &store,
        None,
    )
    .unwrap();
    assert!(
        !imported_with_extra,
        "extra cached files must miss the warm path"
    );
    let expected_blob =
        mgc_store::IntegrityHash::from_bytes(b"module.exports = 'trusted';\n", false);
    assert!(
        !expected_blob.cas_path(store.root()).exists(),
        "a rejected cache must not file CAS claims or import package blobs"
    );
}

#[cfg(unix)]
#[test]
fn warm_cache_file_verification_rejects_a_replaced_symlink() {
    use std::os::unix::fs::symlink;

    let root = tempdir_real().unwrap();
    let outside = tempdir_real().unwrap();
    let cached = root.path().join("entry.js");
    let outside_file = outside.path().join("entry.js");
    std::fs::write(&cached, b"trusted bytes").unwrap();
    std::fs::write(&outside_file, b"trusted bytes").unwrap();
    let metadata = cached_package_files(root.path())
        .unwrap()
        .unwrap()
        .remove(std::path::Path::new("entry.js"))
        .unwrap();

    std::fs::remove_file(&cached).unwrap();
    symlink(&outside_file, &cached).unwrap();

    let matches = cached_archive_file_matches(
        root.path(),
        std::path::Path::new("entry.js"),
        metadata,
        b"trusted bytes",
        false,
    )
    .unwrap();
    assert!(
        !matches,
        "replacing an enumerated file with a symlink must miss the warm cache"
    );
}

#[cfg(unix)]
#[test]
fn warm_cache_file_verification_rejects_a_replaced_parent_directory() {
    use std::os::unix::fs::symlink;

    let root = tempdir_real().unwrap();
    let outside = tempdir_real().unwrap();
    std::fs::create_dir(root.path().join("nested")).unwrap();
    std::fs::write(root.path().join("nested/entry.js"), b"trusted bytes").unwrap();
    std::fs::create_dir(outside.path().join("nested")).unwrap();
    std::fs::write(outside.path().join("nested/entry.js"), b"trusted bytes").unwrap();
    let metadata = cached_package_files(root.path())
        .unwrap()
        .unwrap()
        .remove(std::path::Path::new("nested/entry.js"))
        .unwrap();

    std::fs::rename(
        root.path().join("nested"),
        root.path().join("original-nested"),
    )
    .unwrap();
    symlink(outside.path().join("nested"), root.path().join("nested")).unwrap();

    let matches = cached_archive_file_matches(
        root.path(),
        std::path::Path::new("nested/entry.js"),
        metadata,
        b"trusted bytes",
        false,
    )
    .unwrap();
    assert!(
        !matches,
        "a replaced parent directory must not substitute another cache file"
    );
}

#[cfg(unix)]
#[test]
fn warm_cache_file_verification_rejects_same_inode_directory_moved_outside_root() {
    use std::os::unix::fs::symlink;

    let root = tempdir_real().unwrap();
    let outside = tempdir_real().unwrap();
    let nested = root.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("entry.js"), b"trusted bytes").unwrap();
    let metadata = cached_package_files(root.path())
        .unwrap()
        .unwrap()
        .remove(std::path::Path::new("nested/entry.js"))
        .unwrap();

    let moved = outside.path().join("moved-nested");
    std::fs::rename(&nested, &moved).unwrap();
    symlink(&moved, &nested).unwrap();

    let matches = cached_archive_file_matches(
        root.path(),
        std::path::Path::new("nested/entry.js"),
        metadata,
        b"trusted bytes",
        false,
    )
    .unwrap();
    assert!(
        !matches,
        "a same-inode directory moved outside the cache root must not be reached through a symlink"
    );
}

#[cfg(unix)]
#[allow(unsafe_code)]
#[test]
fn warm_cache_file_verification_does_not_block_on_fifo_replacement() {
    use std::ffi::CString;
    use std::fs::OpenOptions;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let root = tempdir_real().unwrap();
    let cached = root.path().join("entry.js");
    std::fs::write(&cached, b"trusted bytes").unwrap();
    let metadata = cached_package_files(root.path())
        .unwrap()
        .unwrap()
        .remove(std::path::Path::new("entry.js"))
        .unwrap();
    std::fs::remove_file(&cached).unwrap();
    let fifo_path = CString::new(cached.as_os_str().as_bytes()).unwrap();
    // SAFETY: `fifo_path` is a valid NUL-terminated pathname and remains live
    // for the duration of `mkfifo`; the mode is a valid permission mask.
    // (AN TOÀN: `fifo_path` là đường dẫn NUL-terminated hợp lệ và còn sống
    // trong lúc gọi `mkfifo`; mode là mask quyền hợp lệ.)
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);

    let (send, receive) = mpsc::channel();
    let root_path = root.path().to_path_buf();
    std::thread::spawn(move || {
        let result = cached_archive_file_matches(
            &root_path,
            std::path::Path::new("entry.js"),
            metadata,
            b"trusted bytes",
            false,
        );
        let _ = send.send(result);
    });

    match receive.recv_timeout(Duration::from_secs(2)) {
        Ok(matches) => assert!(!matches.unwrap(), "a FIFO is not a cache file"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Unblock a regressed blocking open so this test can fail cleanly.
            // (Mở đầu ghi để giải phóng open bị block nếu regression tái xuất.)
            let _writer = OpenOptions::new().write(true).open(&cached).unwrap();
            assert!(
                receive.recv_timeout(Duration::from_secs(2)).is_ok(),
                "the verifier remained blocked after a FIFO writer connected"
            );
            panic!("cache verification blocked while opening a FIFO")
        }
        Err(error) => panic!("cache verifier thread disconnected: {error}"),
    }
}

#[cfg(windows)]
#[test]
fn warm_cache_opened_file_path_must_stay_beneath_root_handle() {
    let root = Path::new(r"\\?\C:\cache\package");
    assert!(windows_opened_path_is_within(
        root,
        Path::new(r"\\?\C:\cache\package\nested\entry.js")
    ));
    assert!(!windows_opened_path_is_within(
        root,
        Path::new(r"\\?\C:\cache\outside\entry.js")
    ));
    assert!(!windows_opened_path_is_within(
        root,
        Path::new(r"\\?\C:\cache\package-escape\entry.js")
    ));
}

#[test]
fn cached_root_import_falls_back_for_nonstandard_archive_prefix() {
    let cache = tempdir_real().unwrap();
    let package_id = PackageId::new(
        PackageName::new("warm-prefix-probe").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    seed_shared_tarball_with_files(
        cache.path(),
        &package_id,
        &[
            (
                "content/package.json",
                br#"{"name":"warm-prefix-probe","version":"1.0.0"}"#,
            ),
            ("content/index.js", b"module.exports = true;\n"),
        ],
    );
    let tarball = PackageCache::new(cache.path().join("cache"))
        .unwrap()
        .get_tarball(&package_id)
        .unwrap()
        .unwrap();
    let package_root = cache.path().join("existing-package-root");
    std::fs::create_dir_all(&package_root).unwrap();
    let store = ContentStore::new(cache.path().join("cas")).unwrap();

    let imported = mgc_fetcher::extract::extract_tarball_to_cas_and_verify_existing_root(
        std::io::Cursor::new(tarball),
        &package_root,
        &store,
        None,
    )
    .unwrap();

    assert!(!imported);
    assert!(!package_root.join("content").exists());
}
