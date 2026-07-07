#![cfg(test)]

use std::fs;
use std::time::Instant;
use tempfile::tempdir;

use mg_store::{ContentStore, PackageInfo, SqliteStore, StoreIndex};

#[test]
fn benchmark_store_import() {
    let dir = tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src = dir.path().join("data.bin");
    let data = vec![0xABu8; 102_400];
    fs::write(&src, &data).unwrap();

    let mut total = std::time::Duration::ZERO;
    for _ in 0..100 {
        let start = Instant::now();
        store.import_file(&src).unwrap();
        total += start.elapsed();
    }

    let avg = total / 100;
    assert!(
        avg < std::time::Duration::from_micros(2000),
        "avg import time: {:?}",
        avg
    );
}

#[test]
fn benchmark_store_export() {
    let dir = tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src = dir.path().join("data.bin");
    let data = vec![0xCDu8; 102_400];
    fs::write(&src, &data).unwrap();

    let (hash, _) = store.import_file(&src).unwrap();
    let cas_path = store.get_file(&hash).unwrap();

    let mut total = std::time::Duration::ZERO;
    for i in 0..1000 {
        let dest = dir.path().join(format!("out_{}.bin", i));
        let start = Instant::now();
        fs::copy(&cas_path, &dest).unwrap();
        total += start.elapsed();
    }

    let avg = total / 1000;
    assert!(
        avg < std::time::Duration::from_micros(500),
        "avg export time: {:?}",
        avg
    );
}

#[test]
fn benchmark_sqlite_query() {
    let store = SqliteStore::open_in_memory().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for i in 0..1000 {
        let info = PackageInfo {
            name: format!("pkg-{}", i),
            version: "1.0.0".to_string(),
            integrity: format!("{:064x}", i),
            shard: format!("{:02x}", i % 256),
            filename: format!("pkg-{}-1.0.0.tgz", i),
            is_executable: false,
            manifest_json: Some("{}".to_string()),
            metadata: None,
            size_bytes: 1024,
            compressed_size_bytes: 512,
            created_at: now,
        };
        store.add_package(&info).unwrap();
    }

    let mut total = std::time::Duration::ZERO;
    for i in 0..1000 {
        let start = Instant::now();
        let pkg = store.get_package(&format!("pkg-{}", i), "1.0.0").unwrap();
        total += start.elapsed();
        assert!(pkg.is_some());
    }

    let avg = total / 1000;
    assert!(
        avg < std::time::Duration::from_millis(5),
        "avg query time: {:?}",
        avg
    );
}
