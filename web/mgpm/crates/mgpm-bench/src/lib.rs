//! MGPM Benchmarks
//!
//! Criterion-based benchmarks for core mgpm operations.

#[cfg(feature = "mimalloc")]
#[cfg(all(
    any(target_family = "unix", target_family = "windows"),
    not(target_env = "musl"),
    not(miri),
))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

pub fn bench_store_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_import");
    group.sample_size(10);

    for &size in [10, 100].iter() {
        group.bench_with_input(BenchmarkId::new("files", size), &size, |b, &n| {
            b.iter(|| {
                let dir = tempfile::tempdir().unwrap();
                let store = mgpm_store::ContentStore::new(dir.path().join("store")).unwrap();
                let src_dir = dir.path().join("src");
                std::fs::create_dir_all(&src_dir).unwrap();

                for i in 0..n {
                    let path = src_dir.join(format!("file_{}.txt", i));
                    std::fs::write(&path, format!("content {}", i)).unwrap();
                    let _ = store.import_file(&path);
                }
            });
        });
    }
    group.finish();
}

pub fn bench_resolve_basic(c: &mut Criterion) {
    use mgpm_core::PackageName;
    use mgpm_resolver::{DependencyProvider, Resolver, ResolvedDependency};

    struct TestProvider;

    impl DependencyProvider for TestProvider {
        fn get_versions(&self, _package: &PackageName) -> Vec<mgpm_core::Version> {
            vec![
                mgpm_core::Version::parse("1.0.0").unwrap(),
                mgpm_core::Version::parse("2.0.0").unwrap(),
            ]
        }
        fn get_dependencies(
            &self,
            _package_id: &mgpm_core::PackageId,
        ) -> Vec<ResolvedDependency> {
            vec![]
        }
    }

    let mut group = c.benchmark_group("resolver");
    group.sample_size(10);

    for &n in [10, 50].iter() {
        group.bench_with_input(BenchmarkId::new("packages", n), &n, |b, _n| {
            b.iter(|| {
                let resolver = Resolver::new(Box::new(TestProvider));
                let wanted = vec![(
                    PackageName::new("pkg_0").unwrap(),
                    "^1.0.0".to_string(),
                )];
                let _ = resolver.solve(&wanted);
            });
        });
    }
    group.finish();
}

pub fn bench_lockfile_roundtrip(c: &mut Criterion) {
    use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution};

    let dir = tempfile::tempdir().unwrap();

    let mut lock = Lockfile::new(1, "npm");
    for i in 0..100 {
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
            resolved: false,
            resolved_at: None,
        });
    }
    lock.sort_packages();
    lock.compute_content_hash();

    let text_path = dir.path().join("mgpm.lock");
    mgpm_lockfile::text::write_text(&lock, &text_path).unwrap();

    let binary_path = dir.path().join("mgpm.lockb");
    mgpm_lockfile::binary::write_binary(&lock, &binary_path).unwrap();

    let mut group = c.benchmark_group("lockfile_serialize");
    group.sample_size(10);
    group.bench_function("text", |b| {
        b.iter(|| {
            let _ = mgpm_lockfile::text::write_text(&lock, &text_path);
        });
    });
    group.bench_function("binary", |b| {
        b.iter(|| {
            let _ = mgpm_lockfile::binary::write_binary(&lock, &binary_path);
        });
    });
    group.finish();

    let mut group = c.benchmark_group("lockfile_deserialize");
    group.sample_size(10);
    group.bench_function("text", |b| {
        b.iter(|| {
            let _ = mgpm_lockfile::text::read_text(&text_path);
        });
    });
    group.bench_function("binary", |b| {
        b.iter(|| {
            let _ = mgpm_lockfile::binary::read_binary(&binary_path);
        });
    });
    group.finish();
}

criterion_group!(
    name = store;
    config = Criterion::default();
    targets = bench_store_import
);

criterion_group!(
    name = resolver;
    config = Criterion::default();
    targets = bench_resolve_basic
);

criterion_group!(
    name = lockfile;
    config = Criterion::default();
    targets = bench_lockfile_roundtrip
);

criterion_main!(store, resolver, lockfile);
