use criterion::{criterion_group, criterion_main, Criterion};
use mg_types::{
    adapter::InstallOptions, PackageAdapter, PackageId, PackageName, ResolvedGraph,
    ResolvedPackage, Version,
};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;

fn pkg_id(name: &str, version: &str) -> PackageId {
    PackageId::new(
        PackageName::new(name).unwrap(),
        Version::parse(version).unwrap(),
    )
}

fn write_tar_entry(
    builder: &mut tar::Builder<flate2::write::GzEncoder<std::fs::File>>,
    path: &str,
    data: &[u8],
) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, std::io::Cursor::new(data))
        .unwrap();
}

fn make_tarball(dir: &Path, pkg: &PackageId, file_count: usize) {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let store_root = dir.join(".megagate").join("cache").join("web");
    std::fs::create_dir_all(&store_root).unwrap();
    let cache = mg_store::PackageCache::new(store_root.join("cache")).unwrap();
    let tarball_path = cache.tarball_path(pkg);
    if let Some(parent) = tarball_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let file = std::fs::File::create(&tarball_path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let pkg_json = format!(
        "{{\"name\":\"{}\",\"version\":\"{}\"}}",
        pkg.name_str(),
        pkg.version()
    );
    write_tar_entry(&mut builder, "package/package.json", pkg_json.as_bytes());

    for i in 0..file_count {
        let path = format!("package/file-{}.js", i);
        let contents = format!("export const v{i} = {i};");
        write_tar_entry(&mut builder, &path, contents.as_bytes());
    }

    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
}

fn make_tarball_with_files(dir: &Path, pkg: &PackageId, files: &[(&str, &[u8])]) {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let store_root = dir.join(".megagate").join("cache").join("web");
    std::fs::create_dir_all(&store_root).unwrap();
    let cache = mg_store::PackageCache::new(store_root.join("cache")).unwrap();
    let tarball_path = cache.tarball_path(pkg);
    if let Some(parent) = tarball_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let file = std::fs::File::create(&tarball_path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let pkg_json = format!(
        "{{\"name\":\"{}\",\"version\":\"{}\"}}",
        pkg.name_str(),
        pkg.version()
    );
    write_tar_entry(&mut builder, "package/package.json", pkg_json.as_bytes());

    for (path, data) in files {
        write_tar_entry(&mut builder, &format!("package/{}", path), data);
    }

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
                direct: true,
                dev: false,
            })
            .collect(),
    }
}

fn install_all(adapter: &mg_web_adapter::WebAdapter, graph: &ResolvedGraph, dir: &Path) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(adapter.install(graph, dir, InstallOptions::default()))
        .unwrap();
}

/// 1. Large Tree: 100 packages, 10 files each
fn bench_large_tree(c: &mut Criterion) {
    c.bench_function("stress_large_tree_100_pkgs_10_files", |b| {
        b.iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                let pkgs: Vec<PackageId> = (0..100)
                    .map(|i| pkg_id(&format!("pkg-{:03}", i), "1.0.0"))
                    .collect();
                for pkg in &pkgs {
                    make_tarball(dir.path(), pkg, 10);
                }
                let graph = make_graph(&pkgs);
                (dir, graph, pkgs)
            },
            |(dir, graph, pkgs)| {
                let adapter = mg_web_adapter::WebAdapter::new();
                install_all(&adapter, &graph, dir.path());
                for pkg in &pkgs {
                    let f = dir
                        .path()
                        .join("node_modules")
                        .join(pkg.name_str())
                        .join("package.json");
                    assert!(f.exists(), "missing {}", f.display());
                }
            },
        )
    });
}

/// 2. Concurrent installs to same shared cache
fn bench_concurrent_install(c: &mut Criterion) {
    c.bench_function("stress_concurrent_2_parallel", |b| {
        b.iter_with_setup(
            || {
                let pkg = pkg_id("react", "18.2.0");
                let graph = Arc::new(make_graph(&[pkg.clone()]));
                let d1 = Arc::new(tempfile::tempdir().unwrap());
                let d2 = Arc::new(tempfile::tempdir().unwrap());
                make_tarball(d1.path(), &pkg, 3);
                make_tarball(d2.path(), &pkg, 3);
                (graph, d1, d2, pkg)
            },
            |(graph, d1, d2, pkg)| {
                let g1 = graph.clone();
                let g2 = graph;
                let dd1 = d1.clone();
                let dd2 = d2.clone();
                let h1 = std::thread::spawn(move || {
                    let adapter = mg_web_adapter::WebAdapter::new();
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(adapter.install(&g1, dd1.path(), InstallOptions::default()))
                        .unwrap();
                });
                let h2 = std::thread::spawn(move || {
                    let adapter = mg_web_adapter::WebAdapter::new();
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(adapter.install(&g2, dd2.path(), InstallOptions::default()))
                        .unwrap();
                });
                h1.join().unwrap();
                h2.join().unwrap();
                assert!(d1
                    .path()
                    .join("node_modules")
                    .join(pkg.name_str())
                    .join("package.json")
                    .exists());
                assert!(d2
                    .path()
                    .join("node_modules")
                    .join(pkg.name_str())
                    .join("package.json")
                    .exists());
            },
        )
    });
}

/// 3. Corrupted metadata cache recovery
fn bench_corrupted_metadata(c: &mut Criterion) {
    c.bench_function("stress_corrupted_metadata", |b| {
        b.iter_with_setup(
            || {
                let pkg = pkg_id("test-pkg", "1.0.0");
                let dir = tempfile::tempdir().unwrap();
                make_tarball(dir.path(), &pkg, 2);
                let adapter = mg_web_adapter::WebAdapter::new();
                let graph = make_graph(&[pkg]);
                (dir, graph, adapter)
            },
            |(dir, graph, adapter)| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result =
                    rt.block_on(adapter.install(&graph, dir.path(), InstallOptions::default()));
                assert!(result.is_ok(), "install should succeed: {:?}", result.err());
                assert!(dir
                    .path()
                    .join("node_modules")
                    .join("test-pkg")
                    .join("package.json")
                    .exists());
            },
        )
    });
}

/// 4. Deep dependency chain (A→B→C→D→E→F→G)
fn bench_deep_chain(c: &mut Criterion) {
    c.bench_function("stress_deep_chain_7_levels", |b| {
        b.iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                let names = [
                    "base-a", "dep-b", "dep-c", "dep-d", "dep-e", "dep-f", "dep-g",
                ];
                let pkgs: Vec<PackageId> = names.iter().map(|n| pkg_id(n, "1.0.0")).collect();
                for pkg in &pkgs {
                    make_tarball(dir.path(), pkg, 3);
                }
                let graph = make_graph(&pkgs);
                (dir, graph, pkgs)
            },
            |(dir, graph, pkgs)| {
                let adapter = mg_web_adapter::WebAdapter::new();
                install_all(&adapter, &graph, dir.path());
                for pkg in &pkgs {
                    assert!(dir
                        .path()
                        .join("node_modules")
                        .join(pkg.name_str())
                        .join("package.json")
                        .exists());
                }
                let pkg_dir = dir.path().join("node_modules").join("base-a");
                let files: Vec<_> = std::fs::read_dir(&pkg_dir)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .collect();
                if files.len() >= 2 {
                    let m1 = std::fs::metadata(files[0].path()).unwrap();
                    let m2 = std::fs::metadata(files[1].path()).unwrap();
                    assert_ne!(m1.ino(), m2.ino(), "different files should not share inode");
                }
            },
        )
    });
}

/// 5. Reinstall with changed content
fn bench_reinstall_changed(c: &mut Criterion) {
    c.bench_function("stress_reinstall_changed", |b| {
        b.iter_with_setup(
            || {
                let pkg = pkg_id("reinstalled-pkg", "1.0.0");
                let graph = make_graph(&[pkg.clone()]);
                let dir = tempfile::tempdir().unwrap();
                make_tarball_with_files(dir.path(), &pkg, &[("version.txt", b"v1")]);
                let adapter = mg_web_adapter::WebAdapter::new();
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(adapter.install(&graph, dir.path(), InstallOptions::default()))
                    .unwrap();
                (dir, graph, pkg)
            },
            |(dir, _graph, _pkg)| {
                let v_path = dir
                    .path()
                    .join("node_modules")
                    .join("reinstalled-pkg")
                    .join("version.txt");
                let content = std::fs::read(&v_path).unwrap();
                assert_eq!(content, b"v1", "first install should have v1");
            },
        )
    });
}

/// 6. Mixed integrity: some with real integrity, some without
fn bench_mixed_integrity(c: &mut Criterion) {
    c.bench_function("stress_mixed_integrity", |b| {
        b.iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                let pkgs = [pkg_id("mixed-a", "1.0.0"), pkg_id("mixed-b", "1.0.0")];
                for pkg in &pkgs {
                    make_tarball(dir.path(), pkg, 2);
                }
                let cache = mg_store::PackageCache::new(
                    dir.path()
                        .join(".megagate")
                        .join("cache")
                        .join("web")
                        .join("cache"),
                )
                .unwrap();
                let tarball_data = std::fs::read(cache.tarball_path(&pkgs[0])).unwrap();
                let actual_integrity = {
                    use base64::Engine;
                    use sha2::Digest;
                    let hash = sha2::Sha512::digest(&tarball_data);
                    format!(
                        "sha512-{}",
                        base64::engine::general_purpose::STANDARD.encode(hash)
                    )
                };
                let mut graph = make_graph(&pkgs);
                graph.packages[0].integrity = actual_integrity;
                (dir, graph, pkgs)
            },
            |(dir, graph, pkgs)| {
                let adapter = mg_web_adapter::WebAdapter::new();
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result =
                    rt.block_on(adapter.install(&graph, dir.path(), InstallOptions::default()));
                assert!(
                    result.is_ok(),
                    "mixed integrity install: {:?}",
                    result.err()
                );
                for pkg in &pkgs {
                    assert!(dir
                        .path()
                        .join("node_modules")
                        .join(pkg.name_str())
                        .join("package.json")
                        .exists());
                }
            },
        )
    });
}

/// 7. Reinstall after stale cache (simulate lockfile-only install)
fn bench_clean_reinstall(c: &mut Criterion) {
    c.bench_function("stress_clean_reinstall", |b| {
        b.iter_with_setup(
            || {
                let pkg = pkg_id("clean-reinstall", "1.0.0");
                let graph = make_graph(&[pkg.clone()]);
                let dir = tempfile::tempdir().unwrap();
                make_tarball(dir.path(), &pkg, 2);
                let adapter = mg_web_adapter::WebAdapter::new();
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(adapter.install(&graph, dir.path(), InstallOptions::default()))
                    .unwrap();
                std::fs::remove_dir_all(dir.path().join("node_modules")).unwrap();
                (dir, graph, adapter)
            },
            |(dir, graph, adapter)| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(adapter.install(&graph, dir.path(), InstallOptions::default()))
                    .unwrap();
                assert!(dir
                    .path()
                    .join("node_modules")
                    .join("clean-reinstall")
                    .join("package.json")
                    .exists());
            },
        )
    });
}

criterion_group!(
    benches,
    bench_large_tree,
    bench_concurrent_install,
    bench_corrupted_metadata,
    bench_deep_chain,
    bench_reinstall_changed,
    bench_mixed_integrity,
    bench_clean_reinstall,
);
criterion_main!(benches);
