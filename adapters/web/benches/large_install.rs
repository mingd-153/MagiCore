//! Real cached-graph installation benchmark for the Web adapter.
//! Benchmark cài graph đã cache bằng Web adapter thật.
//! This measures MagiCore install/materialization over pre-seeded local tarballs;
//! registry resolution and network transfer are intentionally excluded.
//! Chỉ đo cài/materialize từ tarball local đã seed; không đo resolver hay mạng.

#![allow(clippy::unwrap_used)]

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use flate2::Compression;
use flate2::write::GzEncoder;
use mgc_types::capabilities::ContentStoreProvider;
use mgc_types::{
    PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version, adapter::InstallOptions,
};
use mgc_web_adapter::WebAdapter;
use std::path::Path;
use tar::{Builder, Header};

fn fixture(package_count: usize) -> (tempfile::TempDir, ResolvedGraph) {
    let project = tempfile::tempdir().unwrap();
    let mut dependencies = serde_json::Map::new();
    let mut packages = Vec::with_capacity(package_count);

    for index in 0..package_count {
        let name = format!("mgc-bench-{index}");
        let id = PackageId::new(
            PackageName::new(name.clone()).unwrap(),
            Version::parse("1.0.0").unwrap(),
        );
        dependencies.insert(name, serde_json::Value::String("1.0.0".to_string()));
        seed_cached_tarball(project.path(), &id);
        packages.push(ResolvedPackage {
            id,
            integrity: String::new(),
            tarball_url: String::new(),
            deps: Vec::new(),
            peer_deps: Vec::new(),
            direct: true,
            dev: false,
        });
    }

    std::fs::write(
        project.path().join("package.json"),
        serde_json::json!({
            "name": "mgc-large-install-bench",
            "version": "0.1.0",
            "dependencies": dependencies,
        })
        .to_string(),
    )
    .unwrap();

    (project, ResolvedGraph { packages })
}

fn seed_cached_tarball(root: &Path, package: &PackageId) {
    let cache_root = root.join(".magicore").join("cache").join("web");
    std::fs::create_dir_all(&cache_root).unwrap();
    let cache = mgc_store::PackageCache::new(cache_root.join("cache")).unwrap();
    let tarball = cache.tarball_path(package);
    std::fs::create_dir_all(tarball.parent().unwrap()).unwrap();

    let file = std::fs::File::create(tarball).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = Builder::new(encoder);
    append_file(
        &mut archive,
        &format!(
            "{{\"name\":\"{}\",\"version\":\"1.0.0\"}}",
            package.name_str()
        ),
        "package/package.json",
    );
    append_file(
        &mut archive,
        "export const value = 1;\n",
        "package/index.js",
    );
    archive.finish().unwrap();
    archive.into_inner().unwrap().finish().unwrap();
}

fn append_file(archive: &mut Builder<GzEncoder<std::fs::File>>, contents: &str, path: &str) {
    let mut header = Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, path, contents.as_bytes())
        .unwrap();
}

fn bench_real_cached_install(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Validate every fixture package outside Criterion's timed region.
    // Kiểm tra đủ package của fixture ngoài vùng Criterion đo thời gian.
    for package_count in [10usize, 100, 1_000] {
        let (project, graph) = fixture(package_count);
        let adapter = WebAdapter::new().unwrap();
        let summary = runtime
            .block_on(adapter.install(&graph, project.path(), InstallOptions::default()))
            .unwrap();
        assert_eq!(summary.added.len(), package_count);

        for index in 0..package_count {
            let package_file = project
                .path()
                .join("node_modules")
                .join(format!("mgc-bench-{index}"))
                .join("index.js");
            let contents = std::fs::read_to_string(package_file).unwrap();
            assert_eq!(contents, "export const value = 1;\n");
        }
    }

    let mut group = c.benchmark_group("web_native_cached_graph_install");
    group.sample_size(10);

    for package_count in [10usize, 100, 1_000] {
        group.bench_with_input(
            BenchmarkId::new("direct_packages", package_count),
            &package_count,
            |bencher, &package_count| {
                bencher.iter_batched(
                    || fixture(package_count),
                    |(project, graph)| {
                        let adapter = WebAdapter::new().unwrap();
                        let summary = runtime
                            .block_on(adapter.install(
                                &graph,
                                project.path(),
                                InstallOptions::default(),
                            ))
                            .unwrap();
                        assert_eq!(summary.added.len(), package_count);
                        black_box(summary.duration_ms);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_real_cached_install);
criterion_main!(benches);
