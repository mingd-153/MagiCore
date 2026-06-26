//! Parallel package installer

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;
use tokio::task::JoinSet;

use mgpm_lockfile::Lockfile;
use mgpm_store::{ContentStore, PackageCache};

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub concurrency: usize,
    pub retries: u32,
    pub retry_delay_ms: u64,
    pub store_path: PathBuf,
    pub virtual_store_path: PathBuf,
    pub hoisted_node_modules: bool,
    pub offline: bool,
    pub dry_run: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            concurrency: 16,
            retries: 3,
            retry_delay_ms: 1000,
            store_path: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".mgpm").join("store"),
            virtual_store_path: PathBuf::from(".mgpm").join("virtual_store"),
            hoisted_node_modules: false,
            offline: false,
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub package: String,
    pub phase: InstallPhase,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    Pending, Downloading, Extracting, Linking, Done, Failed, SkippedOffline, SkippedDryRun,
}

pub struct Installer {
    options: InstallOptions,
    store: ContentStore,
    cache: PackageCache,
    progress_tx: mpsc::Sender<InstallProgress>,
}

impl Installer {
    pub fn new(options: InstallOptions, progress_tx: mpsc::Sender<InstallProgress>) -> std::io::Result<Self> {
        let store = ContentStore::new(options.store_path.clone())?;
        let cache = PackageCache::new(options.store_path.join("packages"), options.store_path.join("cache"))?;
        Ok(Self { options, store, cache, progress_tx })
    }

    pub async fn install_lockfile(&self, lockfile: &Lockfile) -> Result<InstallResult, InstallError> {
        let total = lockfile.packages.len();
        let mut join_set = JoinSet::new();

        for pkg in &lockfile.packages {
            let tx = self.progress_tx.clone();
            let pkg_clone = pkg.clone();
            join_set.spawn(async move {
                let _ = tx.send(InstallProgress {
                    package: format!("{}/{}", pkg_clone.name, pkg_clone.version),
                    phase: InstallPhase::Downloading,
                    bytes_downloaded: 0,
                    total_bytes: None,
                }).await;
                Ok(())
            });
        }

        let mut succeeded = 0;
        while let Some(result) = join_set.join_next().await {
            if result.is_ok() { succeeded += 1; }
        }

        Ok(InstallResult { total, succeeded, failed: total - succeeded, skipped: 0, errors: vec![] })
    }
}

impl Clone for Installer {
    fn clone(&self) -> Self {
        Self {
            options: self.options.clone(),
            store: self.store.clone(),
            cache: self.cache.clone(),
            progress_tx: self.progress_tx.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: Vec<InstallError>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum InstallError {
    #[error("store error: {0}")]
    StoreError(String),
    #[error("package not in store")]
    PackageNotInStore,
}
