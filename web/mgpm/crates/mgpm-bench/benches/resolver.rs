//! Resolver benchmarks

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use mgpm_core::{PackageId, PackageName, Version};
use mgpm_resolver::{DependencyProvider, ResolvedDependency, Resolver};

struct BenchDependencyProvider {
    num_versions: usize,
}

impl BenchDependencyProvider {
    fn new(num_versions: usize) -> Self {
        Self { num_versions }
    }
}

impl DependencyProvider for BenchDependencyProvider {
    fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
        (0..self.num_versions)
            .map(|i| Version::parse(&format!("{}.0.0", i)).unwrap())
            .collect()
    }

    fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDependency> {
        vec![]
    }
}

pub fn bench_resolve_basic(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolver");
    group.sample_size(10);

    for &n in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("packages", n), &n, |b, &n| {
            b.iter(|| {
                let provider = BenchDependencyProvider::new(20);
                let resolver = Resolver::new(Box::new(provider));
                let wanted: Vec<(PackageName, String)> = (0..n)
                    .map(|i| {
                        (
                            PackageName::new(&format!("pkg_{}", i)).unwrap(),
                            "^1.0.0".to_string(),
                        )
                    })
                    .collect();
                let _ = resolver.solve(&wanted);
            });
        });
    }
    group.finish();
}

pub fn bench_resolve_catalog(c: &mut Criterion) {
    use mgpm_core::Catalog;
    use std::collections::HashMap;

    let mut group = c.benchmark_group("resolver_catalog");
    group.sample_size(10);

    for &n in [10, 100].iter() {
        group.bench_with_input(BenchmarkId::new("entries", n), &n, |b, &n| {
            b.iter(|| {
                let provider = BenchDependencyProvider::new(5);
                let mut resolver = Resolver::new(Box::new(provider));
                let mut catalog = Catalog::default();
                let mut catalogs = HashMap::new();

                for i in 0..n {
                    catalog.set(&format!("pkg_{}", i), "1.0.0");
                }
                catalogs.insert("default".to_string(), catalog);
                resolver.set_catalogs(catalogs);

                let _ = resolver.resolve_catalog("pkg_0", "default");
            });
        });
    }
    group.finish();
}

criterion_group!(
    name = resolver;
    config = Criterion::default();
    targets = bench_resolve_basic, bench_resolve_catalog
);

criterion_main!(resolver);
