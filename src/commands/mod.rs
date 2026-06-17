use crate::adapters::{self};
use crate::core::cache::Cache;
use crate::core::lock::{load_lock, save_lock};
use anyhow::{Context, Result};

fn print_logo() {
    // Load the ASCII logo from the UI resources and print it to stdout.
    // This provides a visual cue when the user runs `megagate install`.
    const LOGO_PATH: &str = "src/ui/logo.txt";
    if let Ok(content) = std::fs::read_to_string(LOGO_PATH) {
        // Print each line; we keep it simple without color to avoid extra deps.
        // Users can pipe through `cat` if they want colors.
        println!("{}", content);
    } else {
        // Fallback: do nothing if logo file missing.
    }
}

pub async fn install(target: Option<String>) -> Result<()> {
    // Print logo when user runs install
    print_logo();
    let dir = target.unwrap_or_else(|| ".".to_string());
    let adapter = adapters::detect(&dir)?;
    let mut lock = load_lock(&dir)?;
    let changes = adapter.parse(&dir, &mut lock).await?;
    let cache = Cache::new();
    cache.resolve(&changes).await?;
    adapter.install(&dir).await?;
    save_lock(&dir, &lock)?;
    // Also copy the logo file so that users get it alongside the lock
    let _ = std::fs::copy("src/ui/logo.txt", format!("{}/logo.txt", dir));
    Ok(())
}

pub async fn update(package: Option<String>) -> Result<()> {
    let pkg = package.context("Package name required for update")?;
    let dir = ".";
    let adapter = adapters::detect(dir)?;
    adapter.update(dir, &pkg).await?;
    Ok(())
}

pub async fn remove(package: String) -> Result<()> {
    let dir = ".";
    let adapter = adapters::detect(dir)?;
    adapter.remove(dir, &package).await?;
    Ok(())
}

pub async fn list(graph: bool) -> Result<()> {
    if graph {
        println!("Dependency graph (Mermaid):\n```mermaid\nflowchart LR\n    A-->B\n```\n");
    } else {
        println!("List of packages (placeholder)");
    }
    Ok(())
}

pub async fn audit() -> Result<()> {
    println!("Running audit (placeholder)…");
    Ok(())
}

pub async fn export(format: String) -> Result<()> {
    println!("Exporting lock in {} format (placeholder)…", format);
    Ok(())
}
