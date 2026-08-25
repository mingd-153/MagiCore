//! Large install benchmark — Benchmark cài đặt lớn
//! Profiles install performance for 1000 packages

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

/// Benchmark install 10 packages (smoke test)
fn bench_install_10(c: &mut Criterion) {
    c.bench_function("install_10_packages", |b| {
        b.iter(|| {
            // TODO: Real install logic when mg-install API ready
            // For now: simulate work
            let packages = vec!["react", "vue", "angular", "svelte", "solid", "preact", "lit", "alpine", "htmx", "stimulus"];
            for pkg in packages {
                black_box(pkg);
                // Simulate package processing (~5ms per package)
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    });
}

/// Benchmark install 100 packages (medium scale)
fn bench_install_100(c: &mut Criterion) {
    let mut group = c.benchmark_group("install_scale");
    group.sample_size(10); // Fewer samples for longer benchmarks
    
    group.bench_function(BenchmarkId::new("packages", 100), |b| {
        b.iter(|| {
            // TODO: Real install logic
            for i in 0..100 {
                black_box(i);
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    });
    
    group.finish();
}

/// Benchmark install 1000 packages (target scale)
fn bench_install_1000(c: &mut Criterion) {
    let mut group = c.benchmark_group("install_scale");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60)); // Allow longer measurement
    
    group.bench_function(BenchmarkId::new("packages", 1000), |b| {
        b.iter(|| {
            // TODO: Real install logic
            // Target: < 30s for 1000 packages
            for i in 0..1000 {
                black_box(i);
                // Current simulation: 5ms * 1000 = 5s baseline
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    });
    
    group.finish();
}

criterion_group!(benches, bench_install_10, bench_install_100, bench_install_1000);
criterion_main!(benches);
