/// Compare MegaGate vs npm / pnpm / bun on the same packages.
/// Usage: cargo bench -p mg-web-adapter --bench compare
use std::time::{Duration, Instant};
use mg_types::PackageAdapter;

const PACKAGES: &[&str] = &["lodash", "uuid", "dayjs", "axios", "tslib"];
const PKG_JSON: &str = r#"{"name":"cmp","version":"0.1.0","dependencies":{"lodash":"*","uuid":"*","dayjs":"*","axios":"*","tslib":"*"}}"#;

fn run_mg(_label: &str, dir: &std::path::Path) -> (Duration, Duration, Duration) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let adapter = mg_web_adapter::WebAdapter::new();

    let t0 = Instant::now();
    let manifest = rt.block_on(adapter.parse_manifest(dir)).unwrap();
    let graph = rt.block_on(adapter.resolve(&manifest)).unwrap();
    let resolve = t0.elapsed();

    let t1 = Instant::now();
    rt.block_on(adapter.install(&graph, dir)).unwrap();
    let cold = t1.elapsed();

    let t2 = Instant::now();
    rt.block_on(adapter.install(&graph, dir)).unwrap();
    let warm = t2.elapsed();

    (resolve, cold, warm)
}

fn run_npm(dir: &std::path::Path) -> (Duration, Duration) {
    let t0 = Instant::now();
    let out = std::process::Command::new("npm")
        .args(["install", "--no-audit", "--no-fund", "--loglevel=silent"])
        .current_dir(dir)
        .output()
        .expect("npm failed");
    assert!(out.status.success(), "npm install failed: {:?}", String::from_utf8_lossy(&out.stderr));
    let cold = t0.elapsed();

    // clean lockfile to force re-resolve but keep cache
    let _ = std::fs::remove_file(dir.join("package-lock.json"));
    let t1 = Instant::now();
    let out = std::process::Command::new("npm")
        .args(["install", "--no-audit", "--no-fund", "--loglevel=silent", "--prefer-offline"])
        .current_dir(dir)
        .output()
        .expect("npm failed");
    assert!(out.status.success(), "npm warm failed: {:?}", String::from_utf8_lossy(&out.stderr));
    let warm = t1.elapsed();

    (cold, warm)
}

fn run_pnpm(dir: &std::path::Path) -> (Duration, Duration) {
    let t0 = Instant::now();
    let out = std::process::Command::new("pnpm")
        .args(["install", "--no-frozen-lockfile", "--loglevel=silent"])
        .current_dir(dir)
        .output()
        .expect("pnpm failed");
    assert!(out.status.success(), "pnpm install failed: {:?}", String::from_utf8_lossy(&out.stderr));
    let cold = t0.elapsed();

    let _ = std::fs::remove_file(dir.join("pnpm-lock.yaml"));
    let t1 = Instant::now();
    let out = std::process::Command::new("pnpm")
        .args(["install", "--loglevel=silent", "--offline"])
        .current_dir(dir)
        .output()
        .expect("pnpm failed");
    assert!(out.status.success(), "pnpm warm failed: {:?}", String::from_utf8_lossy(&out.stderr));
    let warm = t1.elapsed();

    (cold, warm)
}

fn run_bun(dir: &std::path::Path) -> (Duration, Duration) {
    let t0 = Instant::now();
    let out = std::process::Command::new("bun")
        .args(["install", "--no-save"])
        .current_dir(dir)
        .output()
        .expect("bun failed");
    assert!(out.status.success(), "bun install failed: {:?}", String::from_utf8_lossy(&out.stderr));
    let cold = t0.elapsed();

    let _ = std::fs::remove_file(dir.join("bun.lock"));
    let t1 = Instant::now();
    let out = std::process::Command::new("bun")
        .args(["install", "--no-save"])
        .current_dir(dir)
        .output()
        .expect("bun failed");
    assert!(out.status.success(), "bun warm failed: {:?}", String::from_utf8_lossy(&out.stderr));
    let warm = t1.elapsed();

    (cold, warm)
}

fn count_node_modules(dir: &std::path::Path) -> usize {
    let nm = dir.join("node_modules");
    if !nm.exists() { return 0; }
    std::fs::read_dir(&nm).ok()
        .map(|e| e.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

fn measure_disk(dir: &std::path::Path) -> String {
    let output = std::process::Command::new("du")
        .args(["-sh", dir.join("node_modules").to_str().unwrap_or("")])
        .output()
        .ok();
    if let Some(out) = output {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().split('\t').next().unwrap_or("?").to_string();
        }
    }
    "?".to_string()
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     Package Manager Comparison                             ║");
    println!("║     Packages: {}", format!("{:<48}", PACKAGES.join(", ")));
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let mut results: Vec<(&str, String, String, String, usize, String)> = Vec::new();

    // ── MegaGate ──
    {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), PKG_JSON).unwrap();
        let (resolve, cold, warm) = run_mg("mg", dir.path());
        let count = count_node_modules(dir.path());
        let disk = measure_disk(dir.path());
        println!("┌─ MegaGate");
        println!("│   resolve │ {:.3}s", resolve.as_secs_f64());
        println!("│   install │ cold {:.3}s  warm {:.3}s", cold.as_secs_f64(), warm.as_secs_f64());
        println!("│   packages│ {} │ disk {}", count, disk);
        results.push(("MegaGate",
            format!("{:.3}s", cold.as_secs_f64()),
            format!("{:.3}s", warm.as_secs_f64()),
            format!("{}", count), count, disk));
    }

    // ── npm ──
    {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), PKG_JSON).unwrap();
        let (cold, warm) = run_npm(dir.path());
        let count = count_node_modules(dir.path());
        let disk = measure_disk(dir.path());
        println!("┌─ npm");
        println!("│   install │ cold {:.3}s  warm {:.3}s", cold.as_secs_f64(), warm.as_secs_f64());
        println!("│   packages│ {} │ disk {}", count, disk);
        results.push(("npm",
            format!("{:.3}s", cold.as_secs_f64()),
            format!("{:.3}s", warm.as_secs_f64()),
            format!("{}", count), count, disk));
    }

    // ── pnpm ──
    {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), PKG_JSON).unwrap();
        let (cold, warm) = run_pnpm(dir.path());
        let count = count_node_modules(dir.path());
        let disk = measure_disk(dir.path());
        println!("┌─ pnpm");
        println!("│   install │ cold {:.3}s  warm {:.3}s", cold.as_secs_f64(), warm.as_secs_f64());
        println!("│   packages│ {} │ disk {}", count, disk);
        results.push(("pnpm",
            format!("{:.3}s", cold.as_secs_f64()),
            format!("{:.3}s", warm.as_secs_f64()),
            format!("{}", count), count, disk));
    }

    // ── bun ──
    {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), PKG_JSON).unwrap();
        let (cold, warm) = run_bun(dir.path());
        let count = count_node_modules(dir.path());
        let disk = measure_disk(dir.path());
        println!("┌─ bun");
        println!("│   install │ cold {:.3}s  warm {:.3}s", cold.as_secs_f64(), warm.as_secs_f64());
        println!("│   packages│ {} │ disk {}", count, disk);
        results.push(("bun",
            format!("{:.3}s", cold.as_secs_f64()),
            format!("{:.3}s", warm.as_secs_f64()),
            format!("{}", count), count, disk));
    }

    // ── Summary Table ──
    println!();
    println!("╔══════════╤════════════╤════════════╤═══════════╤══════════╗");
    println!("║ PM       │ Cold       │ Warm       │ Packages  │ Disk     ║");
    println!("╠══════════╪════════════╪════════════╪═══════════╪══════════╣");
    let best_cold = results.iter().filter_map(|(_, c, _, _, _, _)| {
        c.trim_end_matches('s').parse::<f64>().ok()
    }).fold(f64::MAX, |a, b| a.min(b));
    for (name, cold, warm, pkgs, _count, disk) in &results {
        let cold_s = cold.trim_end_matches('s').parse::<f64>().unwrap_or(0.0);
        let marker = if cold_s == best_cold { " 🏆" } else { "" };
        println!("║ {:<8}│ {:<10}│ {:<10}│ {:<9}│ {:<8}║{}",
            name, cold, warm, pkgs, disk, marker);
    }
    println!("╚══════════╧════════════╧════════════╧═══════════╧══════════╝");
    println!();
    println!("🏆 = fastest cold install");
    println!("Cold = first install (no cache)");
    println!("Warm = second install (cached)");
}
