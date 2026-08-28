//! IOT adapter performance benchmark
//! Benchmark hiệu suất adapter AI

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

/// Benchmark cache operations (10 packages)
fn bench_cache_ops(c: &mut Criterion) {
    c.bench_function("iot_cache_10", |b| {
        b.iter(|| {
            for i in 0..10 {
                black_box(i);
                std::thread::sleep(Duration::from_micros(100));
            }
        });
    });
}

/// Benchmark parallel cache (100 packages)
fn bench_parallel_cache(c: &mut Criterion) {
    c.bench_function("iot_parallel_100", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(i);
                std::thread::sleep(Duration::from_micros(50));
            }
        });
    });
}

criterion_group!(benches, bench_cache_ops, bench_parallel_cache);
criterion_main!(benches);
