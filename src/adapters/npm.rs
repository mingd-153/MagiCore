use super::Adapter;
use anyhow::{Result, Context};
use async_trait::async_trait;
use crate::core::lock::{LockFile, PackageRef, DependencyEdge};
use std::process::Command;
use std::path::Path;

#[derive(Default)]
pub struct NpmAdapter;

#[async_trait]
impl Adapter for NpmAdapter {
    async fn parse(&self, dir: &str, lock: &mut LockFile) -> Result<Vec<String>> {
        let manifest_path = Path::new(dir).join("package.json");
        let content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        let deps = json.get("dependencies")
            .and_then(|d| d.as_object())
            .cloned()
            .unwrap_or_default();
        // Populate lock entries
        for (name, ver) in deps.iter() {
            let version = ver.as_str().unwrap_or("*").to_string();
            lock.packages.push(PackageRef {
                name: name.clone(),
                version: version.clone(),
                source: "npm".to_string(),
                integrity: String::new(), // will be filled after fetch
            });
            // No edge information for top‑level deps in this stub
        }
        Ok(vec![]) // no additional items to resolve yet
    }

    async fn install(&self, dir: &str) -> Result<()> {
        // Prefer pnpm if available, otherwise fallback to npm
        let manager = if Command::new("pnpm").arg("--version").output().is_ok() {
            "pnpm"
        } else if Command::new("npm").arg("--version").output().is_ok() {
            "npm"
        } else {
            anyhow::bail!("Neither pnpm nor npm found in PATH")
        };
        let status = Command::new(manager)
            .arg("install")
            .current_dir(dir)
            .status()
            .with_context(|| format!("Failed to run {} install", manager))?;
        if !status.success() {
            anyhow::bail!("{} install failed with code {}", manager, status);
        }
        Ok(())
    }

    async fn update(&self, dir: &str, pkg: &str) -> Result<()> {
        let manager = if Command::new("pnpm").arg("--version").output().is_ok() { "pnpm" } else { "npm" };
        let status = Command::new(manager)
            .args(&["update", pkg])
            .current_dir(dir)
            .status()
            .with_context(|| format!("Failed to run {} update", manager))?;
        if !status.success() {
            anyhow::bail!("{} update failed", manager);
        }
        Ok(())
    }

    async fn remove(&self, dir: &str, pkg: &str) -> Result<()> {
        let manager = if Command::new("pnpm").arg("--version").output().is_ok() { "pnpm" } else { "npm" };
        let status = Command::new(manager)
            .args(&["remove", pkg])
            .current_dir(dir)
            .status()
            .with_context(|| format!("Failed to run {} remove", manager))?;
        if !status.success() {
            anyhow::bail!("{} remove failed", manager);
        }
        Ok(())
    }
}
