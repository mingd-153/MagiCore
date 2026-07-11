use std::time::Instant;
use mg_types::PackageAdapter;

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let total_start = Instant::now();

    let tests = vec![
        ("tiny", serde_json::json!({"is-even": "*", "is-odd": "*", "ansi-styles": "*"})),
        ("medium", serde_json::json!({"semver": "*", "commander": "*", "chalk": "*"})),
        ("real", serde_json::json!({"lodash": "*", "uuid": "*", "dayjs": "*", "axios": "*", "tslib": "*"})),
    ];

    for (label, deps) in &tests {
        println!("\n=== {} ===", label);

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({"name": "cold-test","version":"0.1.0","dependencies": deps}).to_string(),
        ).unwrap();

        let adapter = mg_web_adapter::WebAdapter::new();

        // --- PARSE ---
        let manifest = rt.block_on(adapter.parse_manifest(dir.path())).unwrap();

        // --- RESOLVE ---
        let t0 = Instant::now();
        let graph = rt.block_on(adapter.resolve(&manifest)).unwrap();
        let resolve_time = t0.elapsed();
        println!("  resolve: {} packages in {}.{:03}s",
            graph.packages.len(),
            resolve_time.as_secs(),
            resolve_time.subsec_millis(),
        );

        // --- DOWNLOAD + INSTALL (cold) ---
        let t1 = Instant::now();
        rt.block_on(adapter.install(&graph, dir.path())).unwrap();
        let cold_install = t1.elapsed();
        println!("  cold install: {}.{:03}s", cold_install.as_secs(), cold_install.subsec_millis());

        // verify
        let mut ok = 0;
        for pkg in &graph.packages {
            let pkg_dir = dir.path().join("node_modules").join(pkg.id.name_str());
            if pkg_dir.join("package.json").exists() { ok += 1; }
        }
        println!("  {} / {} packages verified", ok, graph.packages.len());

        // --- WARM INSTALL (2nd run) ---
        let t2 = Instant::now();
        rt.block_on(adapter.install(&graph, dir.path())).unwrap();
        let warm_install = t2.elapsed();
        println!("  warm install: {}.{:03}s", warm_install.as_secs(), warm_install.subsec_millis());
    }

    let total = total_start.elapsed();
    println!("\n=== TOTAL: {}.{:03}s ===", total.as_secs(), total.subsec_millis());
}
