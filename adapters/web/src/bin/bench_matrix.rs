use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use flate2::write::GzEncoder;
use flate2::Compression;
use mg_store::{Layout, PackageCache};
use mg_types::{PackageAdapter, PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version};
use mg_web_adapter::WebAdapter;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tar::{Builder, Header};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioMeasurement {
    name: String,
    runs: usize,
    median_ms: f64,
    avg_ms: f64,
    stddev_ms: f64,
    min_ms: f64,
    max_ms: f64,
    packages: usize,
    bytes_from_cache: u64,
    node_modules_files: usize,
    node_modules_bytes: u64,
    hardlinked_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkBaseline {
    generated_at_epoch_secs: u64,
    scenarios: Vec<ScenarioMeasurement>,
}

#[derive(Debug, Clone, Copy)]
struct NodeModulesStats {
    files: usize,
    bytes: u64,
    hardlinked_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchmarkProfile {
    Standard,
    Heavy,
}

#[derive(Debug, Clone)]
struct FixtureSpec {
    direct_dependencies: Vec<PackageId>,
    graph: ResolvedGraph,
    files_per_package: usize,
    scenario_prefix: &'static str,
}

fn main() {
    if let Err(err) = run() {
        let message = err.to_string();
        if message.contains("Operation not permitted") || message.contains("Permission denied") {
            eprintln!("bench_matrix skipped in restricted sandbox: {message}");
            std::process::exit(0);
        }
        eprintln!("bench_matrix failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    std::env::set_var("MEGAGATE_WEB_ALLOW_INSECURE_LOCALHOST", "1");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut runs = 7usize;
    let mut save_baseline: Option<String> = None;
    let mut compare_baseline: Option<String> = None;
    let mut profile = BenchmarkProfile::Standard;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--runs" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --runs"))?;
                runs = value.parse::<usize>()?;
            }
            "--save-baseline" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --save-baseline"))?;
                save_baseline = Some(value.clone());
            }
            "--compare-baseline" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --compare-baseline"))?;
                compare_baseline = Some(value.clone());
            }
            "--profile" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --profile"))?;
                profile = match value.as_str() {
                    "standard" => BenchmarkProfile::Standard,
                    "heavy" => BenchmarkProfile::Heavy,
                    other => return Err(anyhow::anyhow!("unknown profile: {other}")),
                };
            }
            other => {
                return Err(anyhow::anyhow!("unknown argument: {other}"));
            }
        }
        idx += 1;
    }

    let scenarios = run_matrix(runs, profile)?;
    print_report(&scenarios);

    if let Some(name) = save_baseline {
        let path = baseline_path(&name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        acquire_baseline_lock(&name)?;
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&BenchmarkBaseline {
                generated_at_epoch_secs: current_unix_secs(),
                scenarios: scenarios.clone(),
            })?,
        )?;
        release_baseline_lock(&name)?;
        println!();
        println!("saved baseline: {}", path.display());
    }

    if let Some(name) = compare_baseline {
        acquire_baseline_lock(&name)?;
        let path = baseline_path(&name);
        let baseline: BenchmarkBaseline = serde_json::from_slice(&std::fs::read(&path)?)?;
        release_baseline_lock(&name)?;
        let has_regression = print_comparison(&baseline.scenarios, &scenarios);
        if has_regression {
            eprintln!("⚠️  REGRESSION DETECTED: some scenarios degraded >10% vs baseline");
        }
    }

    Ok(())
}

fn run_matrix(runs: usize, profile: BenchmarkProfile) -> anyhow::Result<Vec<ScenarioMeasurement>> {
    let rt = tokio::runtime::Runtime::new()?;
    let mut scenarios = run_fixture_matrix(&rt, runs, &standard_fixture())?;
    if profile == BenchmarkProfile::Heavy {
        scenarios.extend(run_fixture_matrix(&rt, runs, &heavy_fixture())?);
    }
    Ok(scenarios)
}

fn run_fixture_matrix(
    rt: &tokio::runtime::Runtime,
    runs: usize,
    fixture: &FixtureSpec,
) -> anyhow::Result<Vec<ScenarioMeasurement>> {
    let scenario_name = |suffix: &str| -> String {
        if fixture.scenario_prefix.is_empty() {
            suffix.to_string()
        } else {
            format!("{}-{suffix}", fixture.scenario_prefix)
        }
    };

    let cold_local_cache = measure_scenario(&scenario_name("cold-local-cache"), runs, || {
        let dir = tempfile::tempdir()?;
        write_package_json(dir.path(), &fixture.direct_dependencies)?;
        seed_local_tarballs(
            dir.path(),
            &fixture.graph.packages,
            fixture.files_per_package,
        )?;
        with_isolated_shared_cache(rt, None, || {
            let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
            let started = Instant::now();
            let summary = rt.block_on(adapter.install(
                &fixture.graph,
                dir.path(),
                mg_types::adapter::InstallOptions::default(),
            ))?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let stats = inspect_node_modules(&dir.path().join("node_modules"))?;
            Ok((elapsed, summary.bytes_from_cache, stats))
        })
    })?;

    let warm_reinstall = measure_scenario(&scenario_name("warm-reinstall"), runs, || {
        let dir = tempfile::tempdir()?;
        write_package_json(dir.path(), &fixture.direct_dependencies)?;
        seed_local_tarballs(
            dir.path(),
            &fixture.graph.packages,
            fixture.files_per_package,
        )?;
        with_isolated_shared_cache(rt, None, || {
            let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
            rt.block_on(adapter.install(
                &fixture.graph,
                dir.path(),
                mg_types::adapter::InstallOptions::default(),
            ))?;
            let started = Instant::now();
            let summary = rt.block_on(adapter.install(
                &fixture.graph,
                dir.path(),
                mg_types::adapter::InstallOptions::default(),
            ))?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let stats = inspect_node_modules(&dir.path().join("node_modules"))?;
            Ok((elapsed, summary.bytes_from_cache, stats))
        })
    })?;

    let cold_online = measure_scenario(&scenario_name("cold-online-registry"), runs, || {
        let registry_fixture = rt.block_on(spawn_registry_fixture(
            &fixture.graph.packages,
            fixture.files_per_package,
        ))?;
        let dir = tempfile::tempdir()?;
        write_package_json(dir.path(), &fixture.direct_dependencies)?;
        with_isolated_shared_cache(rt, None, || {
            let adapter = WebAdapter::with_registry(registry_fixture.registry_url.clone());
            let started = Instant::now();
            let manifest = rt.block_on(adapter.parse_manifest(dir.path()))?;
            let graph = rt.block_on(adapter.resolve(&manifest))?;
            let summary = rt.block_on(adapter.install(
                &graph,
                dir.path(),
                mg_types::adapter::InstallOptions::default(),
            ))?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let stats = inspect_node_modules(&dir.path().join("node_modules"))?;
            Ok((elapsed, summary.bytes_from_cache, stats))
        })
    })?;

    let shared_cache_bootstrap =
        measure_scenario(&scenario_name("shared-cache-bootstrap"), runs, || {
            let shared = tempfile::tempdir()?;
            seed_shared_tarballs(
                shared.path(),
                &fixture.graph.packages,
                fixture.files_per_package,
            )?;
            let dir = tempfile::tempdir()?;
            write_package_json(dir.path(), &fixture.direct_dependencies)?;
            with_isolated_shared_cache(rt, Some(shared.path()), || {
                let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
                let started = Instant::now();
                let summary = rt.block_on(adapter.install(
                    &fixture.graph,
                    dir.path(),
                    mg_types::adapter::InstallOptions::default(),
                ))?;
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                let stats = inspect_node_modules(&dir.path().join("node_modules"))?;
                Ok((elapsed, summary.bytes_from_cache, stats))
            })
        })?;

    let offline_cached = measure_scenario(&scenario_name("offline-cached-install"), runs, || {
        let dir = tempfile::tempdir()?;
        write_package_json(dir.path(), &fixture.direct_dependencies)?;
        seed_local_tarballs(
            dir.path(),
            &fixture.graph.packages,
            fixture.files_per_package,
        )?;
        with_isolated_shared_cache(rt, None, || {
            let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
            rt.block_on(adapter.install(
                &fixture.graph,
                dir.path(),
                mg_types::adapter::InstallOptions::default(),
            ))?;
            std::fs::remove_dir_all(dir.path().join("node_modules"))?;
            let started = Instant::now();
            let summary = rt.block_on(adapter.install(
                &fixture.graph,
                dir.path(),
                mg_types::adapter::InstallOptions::default(),
            ))?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let stats = inspect_node_modules(&dir.path().join("node_modules"))?;
            Ok((elapsed, summary.bytes_from_cache, stats))
        })
    })?;

    Ok(vec![
        cold_local_cache.with_packages(fixture.graph.packages.len()),
        warm_reinstall.with_packages(fixture.graph.packages.len()),
        cold_online.with_packages(fixture.graph.packages.len()),
        shared_cache_bootstrap.with_packages(fixture.graph.packages.len()),
        offline_cached.with_packages(fixture.graph.packages.len()),
    ])
}

#[derive(Debug)]
struct ScenarioAccumulator {
    name: String,
    samples_ms: Vec<f64>,
    bytes_from_cache: u64,
    stats: NodeModulesStats,
}

struct RegistryFixture {
    registry_url: String,
}

impl ScenarioAccumulator {
    fn with_packages(self, packages: usize) -> ScenarioMeasurement {
        let mut samples = self.samples_ms;
        samples.sort_by(|a, b| a.partial_cmp(b).expect("non-NaN samples"));
        let median_ms = samples[samples.len() / 2];
        let n = samples.len() as f64;
        let avg_ms = samples.iter().sum::<f64>() / n;
        let variance = samples.iter().map(|s| (s - avg_ms).powi(2)).sum::<f64>() / n;
        let stddev_ms = variance.sqrt();
        let min_ms = *samples.first().expect("non-empty samples");
        let max_ms = *samples.last().expect("non-empty samples");
        ScenarioMeasurement {
            name: self.name,
            runs: samples.len(),
            median_ms,
            avg_ms,
            stddev_ms,
            min_ms,
            max_ms,
            packages,
            bytes_from_cache: self.bytes_from_cache,
            node_modules_files: self.stats.files,
            node_modules_bytes: self.stats.bytes,
            hardlinked_files: self.stats.hardlinked_files,
        }
    }
}

fn measure_scenario<F>(
    name: &str,
    runs: usize,
    mut run_once: F,
) -> anyhow::Result<ScenarioAccumulator>
where
    F: FnMut() -> anyhow::Result<(f64, u64, NodeModulesStats)>,
{
    let mut samples_ms = Vec::with_capacity(runs);
    let mut last_bytes_from_cache = 0u64;
    let mut last_stats = NodeModulesStats {
        files: 0,
        bytes: 0,
        hardlinked_files: 0,
    };

    for _ in 0..runs {
        let (sample_ms, bytes_from_cache, stats) = run_once()?;
        samples_ms.push(sample_ms);
        last_bytes_from_cache = bytes_from_cache;
        last_stats = stats;
    }

    Ok(ScenarioAccumulator {
        name: name.to_string(),
        samples_ms,
        bytes_from_cache: last_bytes_from_cache,
        stats: last_stats,
    })
}

fn standard_fixture() -> FixtureSpec {
    let direct_dependencies = vec![
        package_id("react", "18.2.0"),
        package_id("react-dom", "18.2.0"),
        package_id("vite", "5.4.10"),
        package_id("typescript", "5.5.4"),
        package_id("zod", "3.23.8"),
        package_id("tailwindcss", "4.3.2"),
    ];
    FixtureSpec {
        graph: make_graph(&direct_dependencies, 16),
        direct_dependencies,
        files_per_package: 16,
        scenario_prefix: "",
    }
}

fn heavy_fixture() -> FixtureSpec {
    let react = package_id("react", "18.2.0");
    let react_dom = package_id("react-dom", "18.2.0");
    let vite = package_id("vite", "5.4.10");
    let typescript = package_id("typescript", "5.5.4");
    let tailwind = package_id("tailwindcss", "4.3.2");
    let query = package_id("@tanstack/react-query", "5.59.0");
    let admin_shell = package_id("@workspace/admin-shell", "1.0.0");
    let legacy_tool = package_id("legacy-tool", "1.0.0");

    let scheduler = package_id("scheduler", "0.23.2");
    let design_system = package_id("@workspace/design-system", "1.4.0");
    let react_router = package_id("react-router", "7.0.1");
    let postcss = package_id("postcss", "8.4.47");
    let query_core = package_id("@tanstack/query-core", "5.59.0");
    let semver7 = package_id("semver", "7.8.5");
    let shared_util_v2 = package_id("@workspace/shared-util", "2.1.0");
    let semver6 = package_id("semver", "6.3.1");

    let source_map = package_id("source-map-js", "1.2.1");
    let color_tokens = package_id("@workspace/color-tokens", "1.0.2");
    let icon_pack = package_id("@workspace/icon-pack", "2.3.0");
    let history = package_id("history", "5.3.0");
    let tslib = package_id("tslib", "2.7.0");
    let shared_util_v1 = package_id("@workspace/shared-util", "1.8.4");
    let autoprefixer = package_id("autoprefixer", "10.4.20");
    let browserslist = package_id("browserslist", "4.24.0");
    let caniuse = package_id("caniuse-lite", "1.0.30001667");

    let direct_dependencies = vec![
        react.clone(),
        react_dom.clone(),
        vite.clone(),
        typescript.clone(),
        tailwind.clone(),
        query.clone(),
        admin_shell.clone(),
        legacy_tool.clone(),
    ];

    let graph = ResolvedGraph {
        packages: vec![
            resolved_pkg(react.clone(), vec![scheduler.clone()], true, 48),
            resolved_pkg(
                react_dom.clone(),
                vec![react.clone(), scheduler.clone()],
                true,
                48,
            ),
            resolved_pkg(
                vite.clone(),
                vec![typescript.clone(), semver7.clone()],
                true,
                48,
            ),
            resolved_pkg(typescript.clone(), vec![tslib.clone()], true, 48),
            resolved_pkg(
                tailwind.clone(),
                vec![postcss.clone(), autoprefixer.clone()],
                true,
                48,
            ),
            resolved_pkg(query.clone(), vec![query_core.clone()], true, 48),
            resolved_pkg(
                admin_shell.clone(),
                vec![
                    design_system.clone(),
                    react_router.clone(),
                    shared_util_v2.clone(),
                    color_tokens.clone(),
                ],
                true,
                48,
            ),
            resolved_pkg(
                legacy_tool.clone(),
                vec![shared_util_v1.clone(), semver6.clone()],
                true,
                48,
            ),
            resolved_pkg(scheduler.clone(), vec![], false, 48),
            resolved_pkg(
                design_system.clone(),
                vec![
                    color_tokens.clone(),
                    icon_pack.clone(),
                    shared_util_v1.clone(),
                ],
                false,
                48,
            ),
            resolved_pkg(
                react_router.clone(),
                vec![history.clone(), react.clone()],
                false,
                48,
            ),
            resolved_pkg(
                postcss.clone(),
                vec![source_map.clone(), browserslist.clone()],
                false,
                48,
            ),
            resolved_pkg(query_core.clone(), vec![tslib.clone()], false, 48),
            resolved_pkg(semver7.clone(), vec![], false, 48),
            resolved_pkg(shared_util_v2.clone(), vec![tslib.clone()], false, 48),
            resolved_pkg(semver6.clone(), vec![], false, 48),
            resolved_pkg(source_map.clone(), vec![], false, 48),
            resolved_pkg(color_tokens.clone(), vec![], false, 48),
            resolved_pkg(icon_pack.clone(), vec![shared_util_v1.clone()], false, 48),
            resolved_pkg(history.clone(), vec![], false, 48),
            resolved_pkg(tslib.clone(), vec![], false, 48),
            resolved_pkg(shared_util_v1.clone(), vec![], false, 48),
            resolved_pkg(
                autoprefixer.clone(),
                vec![browserslist.clone(), postcss.clone()],
                false,
                48,
            ),
            resolved_pkg(browserslist.clone(), vec![caniuse.clone()], false, 48),
            resolved_pkg(caniuse.clone(), vec![], false, 48),
        ],
    };

    FixtureSpec {
        direct_dependencies,
        graph,
        files_per_package: 48,
        scenario_prefix: "heavy",
    }
}

async fn spawn_registry_fixture(
    packages: &[ResolvedPackage],
    files_per_package: usize,
) -> anyhow::Result<RegistryFixture> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let registry_url = format!("http://{addr}");
    let package_map = packages
        .iter()
        .map(|pkg| (pkg.id.clone(), pkg))
        .collect::<std::collections::HashMap<_, _>>();
    let mut grouped = std::collections::BTreeMap::<String, Vec<&ResolvedPackage>>::new();
    for pkg in packages {
        grouped
            .entry(pkg.id.name_str().to_string())
            .or_default()
            .push(pkg);
    }

    let metadata_map = Arc::new(grouped.into_iter().map(|(name, versions)| {
        let latest = versions
            .iter()
            .map(|pkg| pkg.id.version().clone())
            .max()
            .expect("versions should be non-empty")
            .to_string();
        let version_entries = versions
            .iter()
            .map(|pkg| {
                let dependencies = if pkg.deps.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(
                        pkg.deps
                            .iter()
                            .map(|dep_id| {
                                let dep_pkg = package_map
                                    .get(dep_id)
                                    .expect("dependency package should exist in fixture graph");
                                (
                                    dep_id.name_str().to_string(),
                                    serde_json::Value::String(format!("^{}", dep_pkg.id.version())),
                                )
                            })
                            .collect(),
                    )
                };
                (
                    pkg.id.version().to_string(),
                    serde_json::json!({
                        "version": pkg.id.version().to_string(),
                        "dependencies": dependencies,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": format!("{registry_url}/tarballs/{}-{}.tgz", sanitize_pkg_name(pkg.id.name_str()), pkg.id.version()),
                            "integrity": null
                        }
                    }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>();

        let metadata = serde_json::json!({
            "name": name,
            "description": null,
            "versions": version_entries,
            "dist-tags": {
                "latest": latest
            }
        });
        (format!("/{}", name), serde_json::to_vec(&metadata).expect("metadata serializable"))
    }).collect::<std::collections::HashMap<_, _>>());
    let tarball_map = Arc::new(
        packages
            .iter()
            .map(|pkg| {
                (
                    format!(
                        "/tarballs/{}-{}.tgz",
                        sanitize_pkg_name(pkg.id.name_str()),
                        pkg.id.version()
                    ),
                    build_tarball_bytes(&pkg.id, files_per_package)
                        .expect("tarball build succeeds"),
                )
            })
            .collect::<std::collections::HashMap<_, _>>(),
    );

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let metadata_map = metadata_map.clone();
            let tarball_map = tarball_map.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let Ok(read) = stream.read(&mut buf).await else {
                    return;
                };
                let request = String::from_utf8_lossy(&buf[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                if let Some(body) = metadata_map.get(path) {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(body).await;
                    return;
                }

                if let Some(body) = tarball_map.get(path) {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(body).await;
                    return;
                }

                let _ = stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
            });
        }
    });

    Ok(RegistryFixture { registry_url })
}

fn package_id(name: &str, version: &str) -> PackageId {
    PackageId::new(
        PackageName::new(name).expect("valid package name"),
        Version::parse(version).expect("valid version"),
    )
}

fn make_graph(packages: &[PackageId], files_per_package: usize) -> ResolvedGraph {
    ResolvedGraph {
        packages: packages
            .iter()
            .cloned()
            .map(|id| resolved_pkg(id, vec![], true, files_per_package))
            .collect(),
    }
}

fn resolved_pkg(
    id: PackageId,
    deps: Vec<PackageId>,
    direct: bool,
    files_per_package: usize,
) -> ResolvedPackage {
    let bytes = build_tarball_bytes(&id, files_per_package)
        .expect("benchmark fixture tarball should build");
    ResolvedPackage {
        id,
        integrity: sri_sha512(&bytes),
        tarball_url: String::new(),
        deps,
        direct,
        dev: false,
    }
}

fn write_package_json(root: &Path, packages: &[PackageId]) -> anyhow::Result<()> {
    let dependencies = packages
        .iter()
        .map(|pkg| {
            (
                pkg.name_str().to_string(),
                serde_json::Value::String(format!("^{}", pkg.version())),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    std::fs::write(
        root.join("package.json"),
        serde_json::json!({
            "name": "mg-bench",
            "version": "0.1.0",
            "private": true,
            "dependencies": dependencies
        })
        .to_string(),
    )?;
    Ok(())
}

fn sanitize_pkg_name(name: &str) -> String {
    name.replace('/', "__").replace('@', "")
}

fn seed_local_tarballs(
    project_root: &Path,
    packages: &[ResolvedPackage],
    files_per_package: usize,
) -> anyhow::Result<()> {
    let layout = Layout::new(project_root.join(".megagate").join("cache").join("web"));
    seed_tarballs_into_layout(&layout, packages, files_per_package)
}

fn seed_shared_tarballs(
    shared_root: &Path,
    packages: &[ResolvedPackage],
    files_per_package: usize,
) -> anyhow::Result<()> {
    let layout = Layout::new(shared_root.to_path_buf());
    seed_tarballs_into_layout(&layout, packages, files_per_package)
}

fn seed_tarballs_into_layout(
    layout: &Layout,
    packages: &[ResolvedPackage],
    files_per_package: usize,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(layout.root())?;
    let cache = PackageCache::new(layout.cache_dir())?;
    for pkg in packages {
        let tarball_path = cache.tarball_path(&pkg.id);
        if let Some(parent) = tarball_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            tarball_path,
            build_tarball_bytes(&pkg.id, files_per_package)?,
        )?;
    }
    Ok(())
}

fn build_tarball_bytes(pkg: &PackageId, files_per_package: usize) -> anyhow::Result<Vec<u8>> {
    let temp = tempfile::NamedTempFile::new()?;
    let file = temp.reopen()?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);

    let package_json = format!(
        "{{\"name\":\"{}\",\"version\":\"{}\"}}",
        pkg.name_str(),
        pkg.version()
    );
    write_tar_entry(
        &mut builder,
        "package/package.json",
        package_json.as_bytes(),
    )?;
    write_tar_entry(
        &mut builder,
        "package/src/index.js",
        format!("export const name = '{}';\n", pkg.name_str()).as_bytes(),
    )?;
    for idx in 0..files_per_package {
        write_tar_entry(
            &mut builder,
            &format!("package/dist/chunk-{idx}.js"),
            format!("export const v{idx} = {idx};\n").as_bytes(),
        )?;
    }

    builder.finish()?;
    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(std::fs::read(temp.path())?)
}

fn sri_sha512(bytes: &[u8]) -> String {
    let digest = Sha512::digest(bytes);
    format!("sha512-{}", BASE64_STANDARD.encode(digest))
}

fn write_tar_entry(
    builder: &mut Builder<GzEncoder<std::fs::File>>,
    path: &str,
    data: &[u8],
) -> anyhow::Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, data)?;
    Ok(())
}

fn inspect_node_modules(root: &Path) -> anyhow::Result<NodeModulesStats> {
    if !root.exists() {
        return Ok(NodeModulesStats {
            files: 0,
            bytes: 0,
            hardlinked_files: 0,
        });
    }

    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut hardlinked_files = 0usize;

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        files += 1;
        let metadata = entry.metadata()?;
        bytes += metadata.len();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() > 1 {
                hardlinked_files += 1;
            }
        }
    }

    Ok(NodeModulesStats {
        files,
        bytes,
        hardlinked_files,
    })
}

fn baseline_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".benchmarks")
        .join(format!("{name}.json"))
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn print_report(scenarios: &[ScenarioMeasurement]) {
    println!("Benchmark matrix (install/materialization only)");
    println!(
        "{:<24} {:>4} {:>10} {:>10} {:>10} {:>10} {:>10} {:>12} {:>10} {:>10} {:>10}",
        "scenario",
        "runs",
        "median",
        "avg",
        "stddev",
        "min",
        "max",
        "cache-bytes",
        "files",
        "nm-bytes",
        "hardlinks"
    );
    for scenario in scenarios {
        println!(
            "{:<24} {:>4} {:>8.2}ms {:>8.2}ms {:>8.2}ms {:>8.2}ms {:>8.2}ms {:>12} {:>10} {:>10} {:>10}",
            scenario.name,
            scenario.runs,
            scenario.median_ms,
            scenario.avg_ms,
            scenario.stddev_ms,
            scenario.min_ms,
            scenario.max_ms,
            scenario.bytes_from_cache,
            scenario.node_modules_files,
            scenario.node_modules_bytes,
            scenario.hardlinked_files,
        );
    }
}

fn print_comparison(previous: &[ScenarioMeasurement], current: &[ScenarioMeasurement]) -> bool {
    println!();
    println!("Baseline comparison");
    println!(
        "{:<24} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "scenario", "prev-med", "curr-med", "delta", "prev-σ", "curr-σ"
    );
    let mut has_regression = false;

    for current_scenario in current {
        let Some(previous_scenario) = previous
            .iter()
            .find(|candidate| candidate.name == current_scenario.name)
        else {
            println!(
                "{:<24} {:>12} {:>12.2}ms {:>12} {:>12} {:>12}",
                current_scenario.name,
                "-",
                current_scenario.median_ms,
                "new",
                "-",
                format!("{:.2}ms", current_scenario.stddev_ms),
            );
            continue;
        };
        let delta_pct = if previous_scenario.median_ms > 0.0 {
            ((current_scenario.median_ms - previous_scenario.median_ms)
                / previous_scenario.median_ms)
                * 100.0
        } else {
            0.0
        };
        if delta_pct > 10.0 {
            has_regression = true;
        }
        let flag = if delta_pct > 10.0 { " ⚠️" } else { "" };
        println!(
            "{:<24} {:>8.2}ms {:>8.2}ms {:>10.2}%{:>4} {:>8.2}ms {:>8.2}ms",
            current_scenario.name,
            previous_scenario.median_ms,
            current_scenario.median_ms,
            delta_pct,
            flag,
            previous_scenario.stddev_ms,
            current_scenario.stddev_ms,
        );
    }

    has_regression
}

fn acquire_baseline_lock(name: &str) -> anyhow::Result<()> {
    let lock_path = baseline_lock_path(name);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let max_attempts = 10usize;
    let retry_delay = std::time::Duration::from_millis(200);
    for attempt in 0..max_attempts {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_file) => {
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale_lock(&lock_path) {
                    let _ = std::fs::remove_file(&lock_path);
                    continue;
                }
                if attempt + 1 < max_attempts {
                    std::thread::sleep(retry_delay);
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("failed to acquire baseline lock: {e}"));
            }
        }
    }
    Err(anyhow::anyhow!(
        "baseline '{name}' is locked by another process (lock exists at {})",
        lock_path.display()
    ))
}

fn release_baseline_lock(name: &str) -> anyhow::Result<()> {
    let lock_path = baseline_lock_path(name);
    let _ = std::fs::remove_file(&lock_path);
    Ok(())
}

fn baseline_lock_path(name: &str) -> PathBuf {
    let mut path = baseline_path(name);
    path.set_extension("lock");
    path
}

fn is_stale_lock(lock_path: &Path) -> bool {
    std::fs::metadata(lock_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() > 60)
}

fn restore_env(key: &str, previous: Option<std::ffi::OsString>) {
    if let Some(value) = previous {
        std::env::set_var(key, value);
    } else {
        std::env::remove_var(key);
    }
}

fn with_isolated_shared_cache<T, F>(
    _rt: &tokio::runtime::Runtime,
    shared_root: Option<&Path>,
    f: F,
) -> anyhow::Result<T>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    let isolated = tempfile::tempdir()?;
    let previous_shared = std::env::var_os("MEGAGATE_SHARED_CACHE_DIR");
    let previous_ttl = std::env::var_os("MEGAGATE_WEB_METADATA_TTL_SECS");
    let previous_retry = std::env::var_os("MEGAGATE_WEB_METADATA_STALE_RETRY_TTL_SECS");
    let previous_max_stale = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");

    std::env::set_var(
        "MEGAGATE_SHARED_CACHE_DIR",
        shared_root.unwrap_or(isolated.path()),
    );
    std::env::set_var("MEGAGATE_WEB_METADATA_TTL_SECS", "300");
    std::env::set_var("MEGAGATE_WEB_METADATA_STALE_RETRY_TTL_SECS", "30");
    std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "604800");

    let result = f();

    restore_env("MEGAGATE_SHARED_CACHE_DIR", previous_shared);
    restore_env("MEGAGATE_WEB_METADATA_TTL_SECS", previous_ttl);
    restore_env("MEGAGATE_WEB_METADATA_STALE_RETRY_TTL_SECS", previous_retry);
    restore_env("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous_max_stale);

    result
}
