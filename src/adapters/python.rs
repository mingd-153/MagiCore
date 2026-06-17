use super::Adapter;
use crate::core::lock::{LockFile, PackageRef};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Command;

#[derive(Default)]
pub struct PythonAdapter;

#[async_trait]
impl Adapter for PythonAdapter {
    /// Parse `requirements.txt` (if present) and add each line as a dependency.
    async fn parse(&self, _dir: &str, lock: &mut LockFile) -> Result<Vec<String>> {
        let path = std::path::Path::new(_dir).join("requirements.txt");
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&path)?;
        let deps: Vec<String> = content
            .lines()
            .filter_map(|l| {
                let trimmed = l.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
        // Populate lock (very simple – just store as version "*")
        for dep in &deps {
            lock.packages.push(PackageRef {
                name: dep.clone(),
                version: "*".to_string(),
                source: "python".to_string(),
                integrity: String::new(),
            });
        }
        Ok(deps)
    }

    async fn install(&self, _dir: &str) -> Result<()> {
        // Run `pip install -r requirements.txt` if the file exists.
        let path = std::path::Path::new(_dir).join("requirements.txt");
        if path.exists() {
            let status = Command::new("pip")
                .arg("install")
                .arg("-r")
                .arg(path)
                .status()?;
            if !status.success() {
                anyhow::bail!("pip install failed");
            }
        }
        Ok(())
    }

    async fn update(&self, _dir: &str, pkg: &str) -> Result<()> {
        // `pip install -U <pkg>`
        let status = Command::new("pip")
            .arg("install")
            .arg("-U")
            .arg(pkg)
            .status()?;
        if !status.success() {
            anyhow::bail!("pip update failed for {}", pkg);
        }
        Ok(())
    }

    async fn remove(&self, _dir: &str, pkg: &str) -> Result<()> {
        // `pip uninstall -y <pkg>`
        let status = Command::new("pip")
            .arg("uninstall")
            .arg("-y")
            .arg(pkg)
            .status()?;
        if !status.success() {
            anyhow::bail!("pip remove failed for {}", pkg);
        }
        Ok(())
    }
}
