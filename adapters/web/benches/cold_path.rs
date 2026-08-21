#![allow(clippy::unwrap_used)]
use criterion::{criterion_group, criterion_main, Criterion};
use mg_types::{
    adapter::InstallOptions, PackageAdapter, PackageId, PackageName, ResolvedGraph,
    ResolvedPackage, Version,
};
use mg_web_adapter::WebAdapter;
use std::path::Path;

fn pkg_id(name: &str, version: &str) -> PackageId {
    PackageId::new(
        PackageName::new(name).unwrap(),
        Version::parse(version).unwrap(),
    )
}

fn seed_cached_tarball(root: &Path, pkg: &PackageId) {
    let store_root = root.join(".megagate").join("cache").join("web");
    std::fs::create_dir_all(&store_root).unwrap();
    let cache = mg_store::PackageCache::new(store_root.join("cache")).unwrap();
    let tarball_path = cache.tarball_path(pkg);
    if let Some(parent) = tarball_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    let file = std::fs::File::create(&tarball_path).unwrap();
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    let pkg_json = format!(
        "{{\"name\":\"{}\",\"version\":\"{}\"}}",
        pkg.name_str(),
        pkg.version()
    );
    header.set_size(pkg_json.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, "package/package.json", pkg_json.as_bytes())
        .unwrap();
    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
}

fn make_graph(packages: &[PackageId]) -> ResolvedGraph {
    ResolvedGraph {
        packages: packages
            .iter()
            .cloned()
            .map(|id| ResolvedPackage {
                id,
                integrity: String::new(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            })
            .collect(),
    }
}

fn bench_cached_install_small(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pkgs = vec![
        pkg_id("is-even", "1.0.0"),
        pkg_id("is-odd", "1.0.0"),
        pkg_id("ansi-styles", "1.0.0"),
    ];
    c.bench_function("cached_install_3_pkgs", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                for pkg in &pkgs {
                    seed_cached_tarball(dir.path(), pkg);
                }
                let graph = make_graph(&pkgs);
                (dir, graph)
            },
            |(dir, graph)| async move {
                let adapter = WebAdapter::new();
                adapter
                    .install(&graph, dir.path(), InstallOptions::default())
                    .await
                    .unwrap();
            },
        )
    });
}

fn bench_cached_install_medium(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pkgs = vec![
        pkg_id("semver", "7.6.0"),
        pkg_id("commander", "12.0.0"),
        pkg_id("chalk", "5.3.0"),
    ];
    c.bench_function("cached_install_medium_3_pkgs", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                for pkg in &pkgs {
                    seed_cached_tarball(dir.path(), pkg);
                }
                let graph = make_graph(&pkgs);
                (dir, graph)
            },
            |(dir, graph)| async move {
                let adapter = WebAdapter::new();
                adapter
                    .install(&graph, dir.path(), InstallOptions::default())
                    .await
                    .unwrap();
            },
        )
    });
}

fn bench_cached_install_real(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pkgs = vec![
        pkg_id("lodash", "4.17.21"),
        pkg_id("uuid", "9.0.0"),
        pkg_id("dayjs", "1.11.10"),
        pkg_id("axios", "1.7.0"),
        pkg_id("tslib", "2.6.0"),
    ];
    c.bench_function("cached_install_real_5_pkgs", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                for pkg in &pkgs {
                    seed_cached_tarball(dir.path(), pkg);
                }
                let graph = make_graph(&pkgs);
                (dir, graph)
            },
            |(dir, graph)| async move {
                let adapter = WebAdapter::new();
                adapter
                    .install(&graph, dir.path(), InstallOptions::default())
                    .await
                    .unwrap();
            },
        )
    });
}

criterion_group!(
    benches,
    bench_cached_install_small,
    bench_cached_install_medium,
    bench_cached_install_real,
);
criterion_main!(benches);
