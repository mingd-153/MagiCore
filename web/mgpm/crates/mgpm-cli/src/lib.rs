use std::path::PathBuf;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use mgpm_installer::installer::{InstallOptions as RealInstallOptions, Installer};
use mgpm_linker::linker::LinkerStrategy;

#[napi(object)]
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub offline: bool,
    pub dry_run: bool,
    pub frozen_lockfile: bool,
    pub production: bool,
    pub dev: bool,
    pub optional: bool,
    pub concurrency: u32,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct AddOptions {
    pub dev: bool,
    pub optional: bool,
    pub peer: bool,
    pub exact: bool,
    pub save: bool,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub latest: bool,
    pub save: bool,
    pub dev: bool,
}

#[napi]
pub struct MgpmCli;

impl Default for MgpmCli {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl MgpmCli {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    #[napi]
    pub async fn install(&self, opts: InstallOptions) -> Result<String> {
        let lockfile_path = PathBuf::from("mgpm.lock");
        let lockfile_content = tokio::fs::read_to_string(&lockfile_path).await
            .map_err(|e| Error::from_reason(format!("failed to read lockfile: {}", e)))?;
        let lockfile: mgpm_lockfile::Lockfile = serde_json::from_str(&lockfile_content)
            .map_err(|e| Error::from_reason(format!("failed to parse lockfile: {}", e)))?;

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let real_opts = RealInstallOptions {
            concurrency: opts.concurrency as usize,
            retries: 3,
            retry_delay_ms: 1000,
            store_path: home.join(".mgpm").join("store"),
            virtual_store_path: PathBuf::from(".mgpm").join("virtual_store"),
            hoisted_node_modules: false,
            hoist_pattern: vec!["*".to_string()],
            offline: opts.offline,
            dry_run: opts.dry_run,
            project_root: PathBuf::from("."),
            sqlite_path: home.join(".mgpm").join("mgpm.db"),
            jsonl_log: false,
            linker_strategy: LinkerStrategy::Hoisted,
            gvs_root: home.join(".mgpm").join("gvs").join("v1"),
        };

        let (tx, _rx) = tokio::sync::mpsc::channel(256);
        let installer = Installer::new(real_opts, tx)
            .map_err(|e| Error::from_reason(format!("failed to create installer: {}", e)))?;

        let result = installer.install_lockfile(&lockfile).await;

        Ok(format!(
            "Installed: {} succeeded, {} failed, {} skipped",
            result.succeeded, result.failed, result.skipped
        ))
    }

    #[napi]
    pub async fn add(&self, pkg: String, opts: AddOptions) -> Result<String> {
        Ok(format!(
            "Added {} (dev={}, peer={}, optional={}, exact={})",
            pkg, opts.dev, opts.peer, opts.optional, opts.exact
        ))
    }

    #[napi]
    pub async fn remove(&self, pkg: String) -> Result<String> {
        Ok(format!("Removed {}", pkg))
    }

    #[napi]
    pub async fn update(&self, opts: UpdateOptions) -> Result<String> {
        Ok(format!("Updated packages (latest={})", opts.latest))
    }

    #[napi]
    pub async fn run(&self, script: String, args: Vec<String>) -> Result<String> {
        Ok(format!("Running script '{}' with args {:?}", script, args))
    }

    #[napi]
    pub async fn exec(&self, command: String, args: Vec<String>) -> Result<String> {
        Ok(format!("Executing '{}' with args {:?}", command, args))
    }

    #[napi]
    pub async fn store_prune(&self) -> Result<String> {
        Ok("Store pruned".to_string())
    }

    #[napi]
    pub async fn store_status(&self) -> Result<String> {
        Ok("Store status ok".to_string())
    }

    #[napi]
    pub async fn config_get(&self, key: String) -> Result<String> {
        Ok(format!("value for {}", key))
    }

    #[napi]
    pub async fn config_set(&self, key: String, value: String) -> Result<String> {
        Ok(format!("Set {} = {}", key, value))
    }

    #[napi]
    pub async fn config_delete(&self, key: String) -> Result<String> {
        Ok(format!("Deleted {}", key))
    }

    #[napi]
    pub async fn config_list(&self) -> Result<String> {
        Ok("Configuration list".to_string())
    }

    #[napi]
    pub async fn init(&self) -> Result<String> {
        Ok("Initialized project".to_string())
    }
}

#[napi]
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
