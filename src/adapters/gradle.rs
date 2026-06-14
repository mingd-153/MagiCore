use super::Adapter;
use anyhow::{Result, Context};
use async_trait::async_trait;
use crate::core::lock::{LockFile, PackageRef, DependencyEdge};
use std::process::Command;
use std::path::Path;

#[derive(Default)]
pub struct GradleAdapter;

#[async_trait]
impl Adapter for GradleAdapter {
    async fn parse(&self, dir: &str, lock: &mut LockFile) -> Result<Vec<String>> {
        // Stub: In a real implementation we would invoke `./gradlew dependencies`
        // and parse the output. For now we just return an empty vec.
        Ok(vec![])
    }

    async fn install(&self, dir: &str) -> Result<()> {
        // Try to run Gradle wrapper if present, otherwise fallback to system gradle
        let wrapper = Path::new(dir).join("gradlew");
        let cmd = if wrapper.exists() { wrapper } else { Path::new("gradle").to_path_buf() };
        let status = Command::new(cmd)
            .arg("build")
            .current_dir(dir)
            .status()
            .with_context(|| "Failed to run gradle build")?;
        if !status.success() {
            anyhow::bail!("Gradle build failed");
        }
        Ok(())
    }

    async fn update(&self, dir: &str, pkg: &str) -> Result<()> {
        // Gradle does not have a simple "update <package>" command.
        anyhow::bail!("Gradle update not supported – edit build.gradle manually")
    }

    async fn remove(&self, _dir: &str, _pkg: &str) -> Result<()> {
        anyhow::bail!("Gradle remove not supported – edit build.gradle manually")
    }
}
