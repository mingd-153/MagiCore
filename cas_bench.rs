use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mgpm_store::{CasContentStore, IntegrityHash};
use mgpm_store::store::sqlite::SqliteStore;
use tempfile::tempdir;

fn create_store() -> CasContentStore {
    let cas_dir = tempdir().unwrap();
    let sqlite = SqliteStore::open_in_memory().unwrap();
    CasContentStore::new(Box::new(sqlite), cas_dir.path().to_path_buf()).unwrap()
}

fn bench_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("import");

    for size in [1024, 1024 * 100, 1024 * 1024, 1024 * 1024 * 10].iter() {
        let data = vec![0x42u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(format!("{}KB", size / 1024)), size, |b, &size| {
            let data = vec![0x42u8; size];
            b.iter(|| {
                let store = create_store();
                store.import_bytes(black_box(&data), false).unwrap()
            });
        });
    }
    group.finish();
}

fn bench_deduplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplication");
    let data = vec![0x42u8; 1024];

    group.bench_function("import_same_content_1000x", |b| {
        b.iter(|| {
            let store = create_store();
            for _ in 0..1000 {
                store.import_bytes(black_box(&data), false).unwrap();
            }
        });
    });
    group.finish();
}

fn bench_export(c: &mut Criterion) {
    let mut group = c.benchmark_group("export");

    for size in [1024, 1024 * 100, 1024 * 1024].iter() {
        let data = vec![0x42u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(format!("{}KB", size / 1024)), size, |b, &size| {
            let store = create_store();
            let data = vec![0x42u8; size];
            let hash = store.import_bytes(&data, false).unwrap();

            let temp = tempdir().unwrap();
            let dest = temp.path().join("out");

            b.iter(|| {
                store.export_to(black_box(&hash), black_box(&dest)).unwrap();
                std::fs::remove_file(&dest).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_verify(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify");

    for size in [1024, 1024 * 100, 1024 * 1024, 1024 * 1024 * 10].iter() {
        let data = vec![0x42u8; *size];
        let store = create_store();
        let hash = store.import_bytes(&data, false).unwrap();
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(format!("{}KB", size / 1024)), size, |b, _| {
            b.iter(|| {
                store.verify(black_box(&hash)).unwrap()
            });
        });
    }
    group.finish();
}

fn bench_concurrent_import(c: &mut Criterion) {
    use std::sync::Arc;
    use std::thread;

    let mut group = c.benchmark_group("concurrent");

    for threads in [1, 2, 4, 8].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(*threads), threads, |b, &threads| {
            b.iter(|| {
                let store = Arc::new(create_store());
                let data = vec![0x42u8; 1024];

                let handles: Vec<_> = (0..threads).map(|_| {
                    let store = Arc::clone(&store);
                    thread::spawn(move || {
                        store.import_bytes(&data, false).unwrap()
                    })
                }).collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        });
    }
    group.finish();
}

fn bench_tarball_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("tarball_batch");

    for count in [10, 100, 1000].iter() {
        let entries: Vec<_> = (0..*count).map(|i| {
            mgpm_store::TarballEntry {
                path: format!("file{}.txt", i),
                data: vec![i as u8; 1024],
                executable: false,
            }
        }).collect();

        group.throughput(Throughput::Elements(*count as u64));

        group.bench_with_input(BenchmarkId::from_parameter(*count), count, |b, _| {
            b.iter(|| {
                let store = create_store();
                store.import_tarball_entries(black_box(&entries)).unwrap()
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_import,
    bench_deduplication,
    bench_export,
    bench_verify,
    bench_concurrent_import,
    bench_tarball_batch
);
criterion_main!(benches);