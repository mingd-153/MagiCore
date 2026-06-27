//! Lockfile benchmarks

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution};

fn create_lockfile(num_packages: usize) -> Lockfile {
    let mut lock = Lockfile::new(1, "https://registry.npmjs.org");
    for i in 0..num_packages {
        lock.add_package(LockfilePackage {
            id: format!("pkg_{}@1.0.0", i),
            name: format!("pkg_{}", i),
            version: "1.0.0".to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: format!(
                    "https://registry.npmjs.org/pkg_{}/-/pkg_{}-1.0.0.tgz",
                    i, i
                ),
                registry: Some("npm".to_string()),
            },
            integrity: Some(format!("sha512-{}", i)),
        });
    }
    lock.sort_packages();
    lock.compute_content_hash();
    lock
}

pub fn bench_lockfile_serialize_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfile_serialize_text");
    group.sample_size(10);

    for &n in [10, 100].iter() {
        let lock = create_lockfile(n);
        let dir = tempfile::tempdir().unwrap();

        group.bench_with_input(BenchmarkId::new("packages", n), &n, |b, _| {
            b.iter(|| {
                let path = dir.path().join("mgpm.lock");
                let _ = mgpm_lockfile::text::write_text(&lock, &path);
            });
        });
    }
    group.finish();
}

pub fn bench_lockfile_deserialize_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfile_deserialize_text");
    group.sample_size(10);

    for &n in [10, 100].iter() {
        let lock = create_lockfile(n);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mgpm.lock");
        mgpm_lockfile::text::write_text(&lock, &path).unwrap();

        group.bench_with_input(BenchmarkId::new("packages", n), &n, |b, _| {
            b.iter(|| {
                let _ = mgpm_lockfile::text::read_text(&path);
            });
        });
    }
    group.finish();
}

pub fn bench_lockfile_serialize_binary(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfile_serialize_binary");
    group.sample_size(10);

    for &n in [10, 100].iter() {
        let lock = create_lockfile(n);
        let dir = tempfile::tempdir().unwrap();

        group.bench_with_input(BenchmarkId::new("packages", n), &n, |b, _| {
            b.iter(|| {
                let path = dir.path().join("mgpm.lockb");
                let _ = mgpm_lockfile::binary::write_binary(&lock, &path);
            });
        });
    }
    group.finish();
}

pub fn bench_lockfile_deserialize_binary(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfile_deserialize_binary");
    group.sample_size(10);

    for &n in [10, 100].iter() {
        let lock = create_lockfile(n);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mgpm.lockb");
        mgpm_lockfile::binary::write_binary(&lock, &path).unwrap();

        group.bench_with_input(BenchmarkId::new("packages", n), &n, |b, _| {
            b.iter(|| {
                let _ = mgpm_lockfile::binary::read_binary(&path);
            });
        });
    }
    group.finish();
}

criterion_group!(
    name = lockfile;
    config = Criterion::default();
    targets = bench_lockfile_serialize_text, bench_lockfile_deserialize_text,
             bench_lockfile_serialize_binary, bench_lockfile_deserialize_binary
);

criterion_main!(lockfile);
