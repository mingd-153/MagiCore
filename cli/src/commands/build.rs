use anyhow::{bail, Result};
use colored::Colorize;
use mg_ui::info;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::bundler::{Bundler, BundlerConfig};
use crate::context::ProjectContext;

pub async fn run(core: Option<&str>) -> Result<()> {
    let root = find_root()?;

    println!("\n📦 {}", "MegaGate Build".bold().cyan());
    info(&format!("Project root: {}", root.display()));

    if root.join("Cargo.toml").exists() {
        return build_rust(&root);
    }

    let ctx = ProjectContext::load_with_core(core)?;
    match ctx.adapter().name() {
        "web" => build_web(&root).await,
        other => bail!("'mg build' not implemented for '{}' core yet", other),
    }
}

fn find_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if let Some(root) = mg_config::project::ProjectConfig::find_project_root(&cwd) {
        return Ok(root);
    }
    anyhow::bail!("No project found (no mg.toml, package.json, or Cargo.toml)")
}

fn build_rust(root: &Path) -> Result<()> {
    let start = Instant::now();
    info("Detected Rust project — running cargo build...");

    let status = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(root)
        .status()?;

    if !status.success() {
        bail!("cargo build failed");
    }

    let elapsed = start.elapsed();
    mg_ui::success(&format!("Rust build completed in {:?}", elapsed));
    Ok(())
}

async fn build_web(root: &Path) -> Result<()> {
    let start_time = Instant::now();
    info("Engine Web: Running native esbuild bundler...");

    let entry = find_entry_point(root)?;
    info(&format!("Entry point: {}", entry.display()));

    let config = BundlerConfig {
        entry: entry.clone(),
        output_dir: root.join("dist"),
        minify: true,
        sourcemap: true,
        target: "es2020".to_string(),
        public_path: "/".to_string(),
    };

    let bundler = Bundler::new(config.clone());
    let result = bundler.bundle().await?;

    let elapsed = start_time.elapsed();
    println!();
    mg_ui::success(&format!(
        "Bundle created: {:.2} KB in {:?}",
        result.size as f64 / 1024.0,
        elapsed
    ));

    info("Processing assets...");
    crate::bundler::process_assets(&config).await?;

    Ok(())
}

fn find_entry_point(root: &Path) -> Result<PathBuf> {
    let candidates = [
        "src/index.ts",
        "src/index.tsx",
        "src/main.ts",
        "src/main.tsx",
        "src/app.ts",
        "src/app.tsx",
        "index.ts",
        "index.tsx",
        "main.ts",
        "main.tsx",
        "src/index.js",
        "src/index.jsx",
        "src/main.js",
        "src/main.jsx",
        "src/app.js",
        "src/app.jsx",
        "index.js",
        "index.jsx",
        "main.js",
        "main.jsx",
    ];

    for candidate in candidates {
        let path = root.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    let pkg_path = root.join("package.json");
    if pkg_path.exists() {
        let content = std::fs::read_to_string(&pkg_path)?;
        let pkg: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
            let path = root.join(main);
            if path.exists() {
                return Ok(path);
            }
        }
        if let Some(module) = pkg.get("module").and_then(|v| v.as_str()) {
            let path = root.join(module);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    bail!("Could not find entry point. Checked: src/index.ts, src/index.tsx, src/main.ts, src/main.tsx, and package.json main/module fields")
}
