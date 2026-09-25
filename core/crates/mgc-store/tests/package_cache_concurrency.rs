#![allow(clippy::unwrap_used)]

use mgc_store::PackageCache;
use mgc_types::PackageId;
use std::sync::{Arc, Barrier};

#[test]
fn concurrent_same_tarball_writers_publish_one_complete_entry() {
    let temp = tempfile::tempdir().unwrap();
    let cache = PackageCache::new(temp.path().join("cache")).unwrap();
    let package = PackageId::parse("same-package@1.2.3").unwrap();
    let payload = Arc::new(vec![b'x'; 256 * 1024]);
    let barrier = Arc::new(Barrier::new(16));

    let writers = (0..16)
        .map(|_| {
            let cache = cache.clone();
            let package = package.clone();
            let payload = Arc::clone(&payload);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                cache.cache_tarball(&package, &payload)
            })
        })
        .collect::<Vec<_>>();

    for writer in writers {
        writer.join().unwrap().unwrap();
    }

    assert_eq!(cache.get_tarball(&package).unwrap().unwrap(), *payload);
    let package_dir = cache.tarball_path(&package).parent().unwrap().to_path_buf();
    assert_eq!(
        std::fs::read_dir(package_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0,
        "concurrent writers must leave no staging files"
    );
}

#[test]
fn conflicting_tarball_winner_is_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let cache = PackageCache::new(temp.path().join("cache")).unwrap();
    let package = PackageId::parse("same-package@1.2.3").unwrap();

    cache.cache_tarball(&package, b"verified winner").unwrap();
    let conflict = cache.cache_tarball(&package, b"different bytes");

    assert!(
        conflict.is_err(),
        "different bytes for one package ID must conflict"
    );
    assert_eq!(
        cache.get_tarball(&package).unwrap().unwrap(),
        b"verified winner"
    );
}
