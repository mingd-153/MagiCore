//! Benchmarks for dependency resolver and version matching.
//!
//! Usage: cargo bench --package mg-resolver

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mg_resolver::{DependencyError, DependencyProvider, Resolver, ResolvedDep};
use mg_types::{PackageId, PackageName, Version, VersionRange};
use std::sync::Arc;

fn make_pkg(name: &str) -> PackageName {
    PackageName::new(name).unwrap()
}

// ─── Providers ──────────────────────────────────────────────────────────────────

struct ManyVersionsProvider(usize);

#[async_trait::async_trait]
impl DependencyProvider for ManyVersionsProvider {
    async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
        let mut v: Vec<Version> = (0..self.0)
            .map(|i| Version::new(1, 0, i as u64))
            .collect();
        v.sort();
        Ok(v)
    }
    async fn get_dependencies(&self, _: &PackageId) -> Result<Vec<ResolvedDep>, DependencyError> {
        Ok(vec![])
    }
}

struct DeepDProvider {
    depth: usize,
    width: usize,
}

#[async_trait::async_trait]
impl DependencyProvider for DeepDProvider {
    async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
        Ok(vec![
            Version::parse("1.0.0").unwrap(),
            Version::parse("2.0.0").unwrap(),
            Version::parse("3.0.0").unwrap(),
        ])
    }
    async fn get_dependencies(&self, id: &PackageId) -> Result<Vec<ResolvedDep>, DependencyError> {
        let name = id.name_str().to_string();
        if let Some(n) = name.strip_prefix("pkg_") {
            let level: usize = n.parse().unwrap_or(0);
            if level < self.depth {
                let deps: Vec<ResolvedDep> = (0..self.width)
                    .map(|i| {
                        let child = format!("pkg_{}_{}", level + 1, i);
                        ResolvedDep {
                            package: make_pkg(&child),
                            spec: "^1.0.0".to_string(),
                            optional: false,
                            peer: false,
                        }
                    })
                    .collect();
                return Ok(deps);
            }
        }
        Ok(vec![])
    }
}

// ─── Benchmarks ─────────────────────────────────────────────────────────────────

fn bench_caret(c: &mut Criterion) {
    let r = VersionRange::parse("^1.2.3").unwrap();
    let v = Version::parse("1.5.0").unwrap();
    c.bench_function("matches_caret", |b| b.iter(|| black_box(r.matches(black_box(&v)))));
}

fn bench_tilde(c: &mut Criterion) {
    let r = VersionRange::parse("~1.2.3").unwrap();
    let v = Version::parse("1.2.9").unwrap();
    c.bench_function("matches_tilde", |b| b.iter(|| black_box(r.matches(black_box(&v)))));
}

fn bench_star(c: &mut Criterion) {
    let r = VersionRange::parse("*").unwrap();
    let v = Version::parse("999.0.0").unwrap();
    c.bench_function("matches_star", |b| b.iter(|| black_box(r.matches(black_box(&v)))));
}

fn bench_or(c: &mut Criterion) {
    let r = VersionRange::parse("^1.0.0 || ^2.0.0").unwrap();
    let v = Version::parse("2.5.0").unwrap();
    c.bench_function("matches_or", |b| b.iter(|| black_box(r.matches(black_box(&v)))));
}

fn bench_sing_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let provider = Arc::new(ManyVersionsProvider(10_000));
    let resolver = Resolver::new(provider);
    let wanted = vec![(make_pkg("react"), "^1.0.0".to_string())];

    c.bench_function("solver_single_10k_versions", |b| {
        b.to_async(&rt).iter(|| resolver.solve(black_box(&wanted)))
    });
}

fn bench_tree_156(c: &mut Criterion) {
    let provider = Arc::new(DeepDProvider { depth: 3, width: 5 });
    let resolver = Resolver::new(provider);
    let wanted = vec![(make_pkg("pkg_0"), "^1.0.0".to_string())];

    c.bench_function("solver_tree_156_pkgs", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| resolver.solve(black_box(&wanted)))
    });
}

criterion_group!(benches_fast, bench_caret, bench_tilde, bench_star, bench_or);
criterion_group!(benches_slow, bench_sing_single, bench_tree_156);
criterion_main!(benches_fast, benches_slow);