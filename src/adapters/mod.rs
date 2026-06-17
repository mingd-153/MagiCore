use crate::core::lock::LockFile;
use anyhow::Result;
use async_trait::async_trait;
use std::any::Any;
#[allow(clippy::default_constructed_unit_structs)]
#[async_trait]
pub trait Adapter: Any {
    /// Parse the manifest(s) found in `dir` and populate `lock`.
    async fn parse(&self, dir: &str, lock: &mut LockFile) -> Result<Vec<String>>;
    /// Run the native install command for the detected manager.
    async fn install(&self, dir: &str) -> Result<()>;
    /// Update a specific package to the latest version.
    async fn update(&self, dir: &str, pkg: &str) -> Result<()>;
    /// Remove a package.
    async fn remove(&self, dir: &str, pkg: &str) -> Result<()>;
}

/// Detect which adapter matches the directory by looking for known manifest files.
#[allow(clippy::default_constructed_unit_structs)]
pub fn detect(dir: &str) -> Result<Box<dyn Adapter>> {
    let path = std::path::Path::new(dir);
    if path.join("package.json").exists() {
        Ok(Box::new(npm::NpmAdapter::default()))
    } else if path.join("Cargo.toml").exists() {
        Ok(Box::new(cargo::CargoAdapter::default()))
    } else if path.join("build.gradle.kts").exists() || path.join("build.gradle").exists() {
        Ok(Box::new(gradle::GradleAdapter::default()))
    } else if path.join("requirements.txt").exists() {
        Ok(Box::new(python::PythonAdapter::default()))
    } else if path.join("bun.lockb").exists() || path.join("pnpm-lock.yaml").exists() {
        Ok(Box::new(webapp::WebAppAdapter::default()))
    } else {
        anyhow::bail!("No supported package manifest found in {}", dir);
    }
}

pub mod cargo;
pub mod gradle;
pub mod npm;
pub mod python;
pub mod webapp;
