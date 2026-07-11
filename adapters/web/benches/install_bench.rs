use criterion::{criterion_group, criterion_main, Criterion};
use flate2::write::GzEncoder;
use flate2::Compression;
use mg_types::{PackageAdapter, PackageId, PackageName, Version};
use mg_web_adapter::WebAdapter;
use std::path::Path;
use tar::{Builder, Header};

fn package_id(name: &str, version: &str) -> PackageId {
    PackageId::new(
        PackageName::new(name).unwrap(),
        Version::parse(version).unwrap(),
    )
}

fn make_graph(packages: &[PackageId]) -> mg_types::ResolvedGraph {
    mg_types::ResolvedGraph {
        packages: packages
            .iter()
            .cloned()
            .map(|id| mg_types::ResolvedPackage {
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

fn seed_cached_tarball(root: &Path, pkg: &PackageId, file_count: usize) {
    let store_root = root.join(".megagate").join("cache").join("web");
    std::fs::create_dir_all(&store_root).unwrap();
    let cache = mg_store::PackageCache::new(store_root.join("cache")).unwrap();
    let tarball_path = cache.tarball_path(pkg);
    if let Some(parent) = tarball_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    let file = std::fs::File::create(&tarball_path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);

    write_tar_entry(
        &mut builder,
        "package/package.json",
        format!(
            "{{\"name\":\"{}\",\"version\":\"{}\"}}",
            pkg.name_str(),
            pkg.version()
        )
        .as_bytes(),
    );

    for idx in 0..file_count {
        let path = format!("package/dist/file-{idx}.js");
        let contents = format!("export const value{idx} = '{idx}';");
        write_tar_entry(&mut builder, &path, contents.as_bytes());
    }

    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
}

fn write_tar_entry(builder: &mut Builder<GzEncoder<std::fs::File>>, path: &str, data: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, data).unwrap();
}

fn bench_cached_install_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("web_install_cached_single_pkg_50_files", |b| {
        b.to_async(&rt).iter(|| async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("package.json"),
                r#"{"name":"bench","version":"0.1.0","dependencies":{"tailwindcss":"^4.3.0"}}"#,
            )
            .unwrap();

            let pkg = package_id("tailwindcss", "4.3.2");
            seed_cached_tarball(dir.path(), &pkg, 50);

            let graph = make_graph(std::slice::from_ref(&pkg));
            let adapter = WebAdapter::new();
            adapter.install(&graph, dir.path()).await.unwrap();
        })
    });
}

fn bench_cached_install_multi(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("web_install_cached_5_pkgs_20_files_each", |b| {
        b.to_async(&rt).iter(|| async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("package.json"),
                r#"{"name":"bench","version":"0.1.0","dependencies":{"react":"^18.2.0","tailwindcss":"^4.3.0","zod":"^3.23.0","vite":"^5.4.0","typescript":"^5.5.0"}}"#,
            )
            .unwrap();

            let packages = vec![
                package_id("react", "18.2.0"),
                package_id("tailwindcss", "4.3.2"),
                package_id("zod", "3.23.8"),
                package_id("vite", "5.4.10"),
                package_id("typescript", "5.5.4"),
            ];

            for pkg in &packages {
                seed_cached_tarball(dir.path(), pkg, 20);
            }

            let graph = make_graph(&packages);
            let adapter = WebAdapter::new();
            adapter.install(&graph, dir.path()).await.unwrap();
        })
    });
}

fn bench_cached_install_stress(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("web_install_cached_25_pkgs_40_files_each", |b| {
        b.to_async(&rt).iter(|| async {
            let dir = tempfile::tempdir().unwrap();

            let mut deps = serde_json::Map::new();
            let mut packages = Vec::new();
            for idx in 0..25 {
                let name = format!("pkg-{idx}");
                deps.insert(
                    name.clone(),
                    serde_json::Value::String("^1.0.0".to_string()),
                );
                packages.push(package_id(&name, "1.0.0"));
            }

            std::fs::write(
                dir.path().join("package.json"),
                serde_json::json!({
                    "name": "bench",
                    "version": "0.1.0",
                    "dependencies": deps
                })
                .to_string(),
            )
            .unwrap();

            for pkg in &packages {
                seed_cached_tarball(dir.path(), pkg, 40);
            }

            let graph = make_graph(&packages);
            let adapter = WebAdapter::new();
            adapter.install(&graph, dir.path()).await.unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_cached_install_single,
    bench_cached_install_multi,
    bench_cached_install_stress
);
criterion_main!(benches);
