use anyhow::{Result, Context};
use crate::adapters::{self, Adapter};
use crate::core::lock::{LockFile, load_lock, save_lock};
use crate::core::cache::Cache;
use agent_memory::trellis::Trellis;

pub async fn install(target: Option<String>) -> Result<()> {
    let dir = target.unwrap_or_else(|| ".".to_string());
    let adapter = adapters::detect(&dir)?;
    let mut lock = load_lock(&dir)?;
    let changes = adapter.parse(&dir, &mut lock).await?;
    let cache = Cache::new();
    cache.resolve(&changes).await?;
    adapter.install(&dir).await?;
    save_lock(&dir, &lock)?;
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

pub async fn recall(key: String) -> Result<()> {
    match Trellis::fetch(&key)? {
        Some(value) => println!("[Recall] {} = {}", key, value),
        None => println!("[Recall] key '{}' not found", key),
    }
    Ok(())
}
