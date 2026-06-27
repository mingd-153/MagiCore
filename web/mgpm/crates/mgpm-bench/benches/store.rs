//! Content store benchmarks

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

pub fn bench_store_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_import");
    group.sample_size(10);

    for &size in &[1024usize, 10240, 102400] {
        group.bench_with_input(BenchmarkId::new("bytes", size), &size, |b, &n| {
            b.iter(|| {
                let dir = tempfile::tempdir().unwrap();
                let store = mgpm_store::ContentStore::new(dir.path().join("store")).unwrap();
                let data = vec![0xABu8; n];
                let path = dir.path().join("file.bin");
                std::fs::write(&path, &data).unwrap();
                let _ = store.import_file(&path);
            });
        });
    }
    group.finish();
}

pub fn bench_store_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_hash");
    group.sample_size(10);

    for &size in [1024, 65536].iter() {
        group.bench_with_input(BenchmarkId::new("bytes", size), &size, |b, &n| {
            let dir = tempfile::tempdir().unwrap();
            let store = mgpm_store::ContentStore::new(dir.path().join("store")).unwrap();
            let data = vec![0xABu8; n];
            let path = dir.path().join("data.bin");
            std::fs::write(&path, &data).unwrap();

            b.iter(|| {
                let _ = store.hash_file(&path);
            });
        });
    }
    group.finish();
}

criterion_group!(
    name = store;
    config = Criterion::default();
    targets = bench_store_import, bench_store_hash
);

criterion_main!(store);
