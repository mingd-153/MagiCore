use crate::strategy::{HardlinkStrategy, LinkStrategy, SymlinkStrategy, CopyStrategy};
use megagate_types::error::{MegagateError, Result};
use megagate_types::lockfile::LockfileV1;
use megagate_types::package::PackageRef;
use megagate_types::config::LinkStrategy as ConfigLinkStrategy;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Linker {
    store: Arc<dyn megagate_types::store::StoreBackend>,
    strategy: Arc<dyn LinkStrategy>,
}

impl Linker {
    pub fn new(store: Arc<dyn megagate_types::store::StoreBackend>, config_strategy: ConfigLinkStrategy) -> Self {
        let strategy: Arc<dyn LinkStrategy> = match config_strategy {
            ConfigLinkStrategy::Hardlink => Arc::new(HardlinkStrategy),
            ConfigLinkStrategy::Symlink => Arc::new(SymlinkStrategy),
            ConfigLinkStrategy::Copy => Arc::new(CopyStrategy),
        };
        Self { store, strategy }
    }

    pub async fn link(&self, importer_path: &PathBuf, lockfile: &LockfileV1) -> Result<()> {
        let virtual_store = importer_path.join("node_modules").join(".megagate");
        tokio::fs::create_dir_all(&virtual_store).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;

        for (key, pkg) in &lockfile.packages {
            let pkg_ref = PackageRef::new(pkg.name.clone(), pkg.version.clone());
            let virtual_pkg = virtual_store.join(key);
            
            eprintln!("DEBUG: Linking {} to {}", key, virtual_pkg.display());
            
            self.strategy.link(&pkg_ref, &virtual_pkg).await?;
            eprintln!("DEBUG: Strategy link done for {}", key);

            let node_modules_link = importer_path.join("node_modules").join(&pkg.name);
            if node_modules_link.exists() {
                tokio::fs::remove_file(&node_modules_link).await.ok();
            }
            eprintln!("DEBUG: Creating symlink from {} to {}", node_modules_link.display(), virtual_pkg.display());
            tokio::fs::symlink(&virtual_pkg, &node_modules_link).await
                .map_err(|e| {
                    eprintln!("DEBUG: Symlink error: {}", e);
                    MegagateError::IoError(e.to_string())
                })?;
        }

        self.link_transitive_deps(&virtual_store, lockfile).await
    }

    async fn link_transitive_deps(&self, virtual_store: &PathBuf, lockfile: &LockfileV1) -> Result<()> {
        for (key, pkg) in &lockfile.packages {
            let pkg_store_path = self.store.get_path(&PackageRef::new(pkg.name.clone(), pkg.version.clone())).await?;
            let pkg_node_modules = pkg_store_path.join("node_modules");
            tokio::fs::create_dir_all(&pkg_node_modules).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;

            for (dep_name, dep_version) in &pkg.dependencies {
                if let Some(dep_pkg) = lockfile.get_package(dep_name, &semver::Version::parse(dep_version).unwrap()) {
                    let dep_ref = PackageRef::new(dep_name.clone(), dep_pkg.version.clone());
                    let dep_virtual = virtual_store.join(format!("{}@{}", dep_name, dep_version));
                    self.strategy.link(&dep_ref, &dep_virtual).await?;

                    let dep_link = pkg_node_modules.join(dep_name);
                    tokio::fs::symlink(&dep_virtual, &dep_link).await
                        .map_err(|e| MegagateError::IoError(e.to_string()))?;
                }
            }
        }
        Ok(())
    }

    pub async fn unlink_package(&self, importer_path: &PathBuf, pkg_name: &str) -> Result<()> {
        let node_modules_link = importer_path.join("node_modules").join(pkg_name);
        if node_modules_link.exists() {
            tokio::fs::remove_file(&node_modules_link).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn clean(&self, importer_path: &PathBuf) -> Result<()> {
        let node_modules = importer_path.join("node_modules");
        if node_modules.exists() {
            tokio::fs::remove_dir_all(node_modules).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
        }
        Ok(())
    }
}