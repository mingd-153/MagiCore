use super::Adapter;
use anyhow::{Result, Context};
use async_trait::async_trait;
use crate::core::lock::{LockFile, PackageRef};
use std::process::Command;
use std::path::Path;

#[derive(Default)]
pub struct WebAppAdapter;

#[async_trait]
impl Adapter for WebAppAdapter {
    // Parse package.json (common for both bun and pnpm)
    async fn parse(&self, dir: &str, lock: &mut LockFile) -> Result<Vec<String>> {
        let manifest_path = Path::new(dir).join("package.json");
        let content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        let deps = json
            .get("dependencies")
            .and_then(|d| d.as_object())
            .cloned()
            .unwrap_or_default();
        for (name, ver) in deps.iter() {
            let version = ver.as_str().unwrap_or("*").to_string();
            // Use "webapp" as source to indicate it may be managed by bun or pnpm
            lock.packages.push(PackageRef {
                name: name.clone(),
                version: version.clone(),
                source: "webapp".to_string(),
                integrity: String::new(),
            });
        }
        Ok(vec![])
    }


    async fn install(&self, dir: &str) -> Result<()> {
        let manager = Self::choose_manager()?;
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
        let manager = Self::choose_manager()?;
        let status = match manager {
            "bun" => {
                // bun: `bun add <pkg>@latest` or `bun upgrade` for all
                let mut cmd = Command::new("bun");
                if pkg.is_empty() {
                    cmd.arg("upgrade");
                } else {
                    cmd.args(["add", &format!("{}@latest", pkg)]);
                }
                cmd.current_dir(dir).status()
            }
            "pnpm" => {
                let mut cmd = Command::new("pnpm");
                if pkg.is_empty() {
                    cmd.arg("update");
                } else {
                    cmd.args(["update", pkg]);
                }
                cmd.current_dir(dir).status()
            }
            "npm" => {
                let mut cmd = Command::new("npm");
                if pkg.is_empty() {
                    cmd.arg("update");
                } else {
                    cmd.args(["update", pkg]);
                }
                cmd.current_dir(dir).status()
            }
        _ => unreachable!(),
    }.with_context(|| format!("Failed to run {} update", manager))?;
        if !status.success() {
            anyhow::bail!("{} update failed with code {}", manager, status);
        }
        Ok(())
    }

    async fn remove(&self, dir: &str, pkg: &str) -> Result<()> {
        let manager = Self::choose_manager()?;
        let status = Command::new(manager)
            .args(["remove", pkg])
            .current_dir(dir)
            .status()
            .with_context(|| format!("Failed to run {} remove", manager))?;
        if !status.success() {
            anyhow::bail!("{} remove failed with code {}", manager, status);
        }
        Ok(())
    }
}

impl WebAppAdapter {
    fn choose_manager() -> Result<&'static str> {
        if Command::new("bun").arg("--version").output().is_ok() {
            Ok("bun")
        } else if Command::new("pnpm").arg("--version").output().is_ok() {
            Ok("pnpm")
        } else if Command::new("npm").arg("--version").output().is_ok() {
            Ok("npm")
        } else {
            anyhow::bail!("Neither bun, pnpm nor npm found in PATH")
        }
    }
}
