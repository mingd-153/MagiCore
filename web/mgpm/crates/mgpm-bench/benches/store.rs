//! Comprehensive store benchmarks: SQLite vs ContentStore, bulk, query, concurrent

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, black_box};
use mgpm_store::{SqliteStore, ContentStore, StoreIndex, PackageInfo};
use std::time::Duration;
use std::sync::Arc;
use std::thread;

fn make_pkg(i: usize) -> PackageInfo {
    PackageInfo {
        name: format!("pkg-{}", i % 1000),
        version: format!("{}.{}.{}", i / 1000, (i / 100) % 10, i % 100),
        integrity: format!("sha256-{:064x}", i),
        shard: format!("{:02x}/{:064x}", i % 256, i),
        filename: format!("pkg-{}-{}.{}.{}.tgz", i % 1000, i / 1000, (i / 100) % 10, i % 100),
        is_executable: false,
        manifest_json: Some(format!(r#"{{"name":"pkg-{}","version":"1.0.0"}}"#, i)),
        metadata: None,
        size_bytes: 1024 + (i as u64),
        compressed_size_bytes: 512 + (i as u64 / 2),
        created_at: 1000000 + (i as u64),
    }
}

// ── SQLite open/create ──

fn bench_sqlite_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_open");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("create_new", |b| {
        b.iter(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("index.db");
            let store = SqliteStore::open(&path, false).unwrap();
            black_box(store);
        });
    });

    group.bench_function("open_existing", |b| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        SqliteStore::open(&path, false).unwrap();
        b.iter(|| {
            let store = SqliteStore::open(&path, false).unwrap();
            black_box(store);
        });
    });

    group.bench_function("open_readonly", |b| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        SqliteStore::open(&path, false).unwrap();
        b.iter(|| {
            let store = SqliteStore::open(&path, true).unwrap();
            black_box(store);
        });
    });

    group.bench_function("in_memory", |b| {
        b.iter(|| {
            let store = SqliteStore::open_in_memory().unwrap();
            black_box(store);
        });
    });

    group.finish();
}

// ── SQLite bulk add ──

fn bench_sqlite_bulk_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_bulk_add");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));

    for &count in &[100, 500, 1000] {
        group.bench_with_input(BenchmarkId::new("packages", count), &count, |b, &n| {
            b.iter(|| {
                let dir = tempfile::tempdir().unwrap();
                let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
                store.begin_transaction().unwrap();
                for i in 0..n {
                    store.add_package(&make_pkg(i)).unwrap();
                }
                store.commit().unwrap();
            });
        });
    }

    group.finish();
}

// ── SQLite query ──

fn bench_sqlite_query(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();

    // Insert 1000 packages
    store.begin_transaction().unwrap();
    for i in 0..1000 {
        store.add_package(&make_pkg(i)).unwrap();
    }
    store.commit().unwrap();

    let mut group = c.benchmark_group("sqlite_query");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("by_name_version", |b| {
        b.iter(|| {
            let pkg = store.get_package("pkg-42", "0.0.42").unwrap();
            black_box(pkg);
        });
    });

    group.bench_function("by_integrity", |b| {
        b.iter(|| {
            let pkg = store.get_by_integrity("sha256-0000000000000000000000000000000000000000000000000000000000000042").unwrap();
            black_box(pkg);
        });
    });

    group.bench_function("exists_hit", |b| {
        b.iter(|| {
            let exists = store.package_exists("sha256-0000000000000000000000000000000000000000000000000000000000000042").unwrap();
            black_box(exists);
        });
    });

    group.bench_function("exists_miss", |b| {
        b.iter(|| {
            let exists = store.package_exists("sha256-nonexistent").unwrap();
            black_box(exists);
        });
    });

    group.bench_function("count", |b| {
        b.iter(|| {
            let count = store.package_count().unwrap();
            black_box(count);
        });
    });

    group.finish();
}

// ── SQLite KV operations ──

fn bench_sqlite_kv(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_kv");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("set_1kb", |b| {
        b.iter(|| {
            let dir = tempfile::tempdir().unwrap();
            let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
            for i in 0..100 {
                store.set_kv(&format!("key-{}", i), &vec![0u8; 1024]).unwrap();
            }
        });
    });

    group.bench_function("get_sequential", |b| {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
        for i in 0..100 {
            store.set_kv(&format!("key-{}", i), &vec![0u8; 1024]).unwrap();
        }
        b.iter(|| {
            for i in 0..100 {
                let val = store.get_kv(&format!("key-{}", i)).unwrap();
                black_box(val);
            }
        });
    });

    group.finish();
}

// ── SQLite GC / generation ──

fn bench_sqlite_gc(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_gc");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));

    group.bench_function("advance_generation", |b| {
        b.iter(|| {
            let dir = tempfile::tempdir().unwrap();
            let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
            store.begin_transaction().unwrap();
            for i in 0..1000 {
                store.add_package(&make_pkg(i)).unwrap();
            }
            store.commit().unwrap();
            store.advance_generation().unwrap();
            let orphaned = store.get_unreferenced_packages().unwrap();
            black_box(orphaned.len());
        });
    });

    group.finish();
}

// ── SQLite concurrent access ──

fn bench_sqlite_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_concurrent");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));

    group.bench_function("4_threads_read", |b| {
        b.iter(|| {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(SqliteStore::open(&dir.path().join("index.db"), false).unwrap());
            store.begin_transaction().unwrap();
            for i in 0..100 {
                store.add_package(&make_pkg(i)).unwrap();
            }
            store.commit().unwrap();

            let store = Arc::new(SqliteStore::open(&dir.path().join("index.db"), true).unwrap());
            let mut handles = vec![];
            for _ in 0..4 {
                let s = store.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..100 {
                        let _ = s.get_package(&format!("pkg-{}", i % 100), &format!("0.0.{}", i));
                    }
                }));
            }
            for h in handles { h.join().unwrap(); }
        });
    });

    group.finish();
}

// ── ContentStore vs SQLite import comparison ──

fn bench_store_import_content(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_import_content_vs_sqlite");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));

    for &count in &[10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("ContentStore_files", count),
            &count,
            |b, &n| {
                b.iter(|| {
                    let dir = tempfile::tempdir().unwrap();
                    let store = ContentStore::new(dir.path().join("store")).unwrap();
                    let src = dir.path().join("src");
                    std::fs::create_dir_all(&src).unwrap();
                    for i in 0..n {
                        let path = src.join(format!("f{}.txt", i));
                        std::fs::write(&path, format!("content {}", i)).unwrap();
                        let _ = store.import_file(&path);
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_sqlite_health(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_health");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("health_check", |b| {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
        store.begin_transaction().unwrap();
        for i in 0..100 {
            store.add_package(&make_pkg(i)).unwrap();
        }
        store.commit().unwrap();
        b.iter(|| {
            let report = store.health_check().unwrap();
            black_box(report);
        });
    });

    group.bench_function("vacuum", |b| {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
        store.begin_transaction().unwrap();
        for i in 0..100 {
            store.add_package(&make_pkg(i)).unwrap();
        }
        store.commit().unwrap();
        b.iter(|| {
            store.vacuum().unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    name = store_sqlite;
    config = Criterion::default().warm_up_time(Duration::from_secs(1)).sample_size(10);
    targets = bench_sqlite_open, bench_sqlite_bulk_add, bench_sqlite_query,
              bench_sqlite_kv, bench_sqlite_gc, bench_sqlite_concurrent,
              bench_sqlite_health
);

criterion_group!(
    name = store_content;
    config = Criterion::default().warm_up_time(Duration::from_secs(1));
    targets = bench_store_import_content
);

criterion_main!(store_sqlite, store_content);
