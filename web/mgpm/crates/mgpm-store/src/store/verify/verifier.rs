use std::fs;

use tracing;

use super::report::StoreReport;
use super::utils::verify_file_integrity;
use crate::store::cas::ContentStore;
use crate::store::index::{StoreError, StoreIndex};

pub struct StoreVerifier<'a> {
    store: &'a ContentStore,
    index: &'a dyn StoreIndex,
}

impl<'a> StoreVerifier<'a> {
    pub fn new(store: &'a ContentStore, index: &'a dyn StoreIndex) -> Self {
        Self { store, index }
    }

    pub fn verify(&self, fix: bool) -> Result<StoreReport, StoreError> {
        if fix {
            if self.index.is_readonly() {
                return Err(StoreError::Database(
                    "cannot fix: store index is readonly".into(),
                ));
            }
            if !self.index.check_integrity()? {
                return Err(StoreError::Database(
                    "index integrity check failed: refusing --fix on corrupt database".into(),
                ));
            }
        }

        let start = std::time::Instant::now();
        let mut report = StoreReport::default();

        let packages = self.index.get_all_packages()?;
        report.total_packages = packages.len() as u64;

        for pkg in &packages {
            let cas_path = self
                .store
                .root()
                .join(&pkg.shard)
                .join(&pkg.filename);

            if !cas_path.exists() {
                report.missing_files.push(pkg.integrity.clone());
                if fix {
                    tracing::warn!("cannot auto-fix missing file: {}", pkg.integrity);
                }
                continue;
            }

            match verify_file_integrity(&cas_path, &pkg.integrity) {
                Ok(true) => {
                    report.verified += 1;
                }
                Ok(false) => {
                    report.corrupted_files.push(pkg.integrity.clone());
                    if fix {
                        tracing::info!("re-importing corrupted file: {}", pkg.integrity);
                        let _ = fs::remove_file(&cas_path);
                    }
                }
                Err(e) => {
                    tracing::error!("error verifying {}: {}", pkg.integrity, e);
                    report.corrupted_files.push(pkg.integrity.clone());
                }
            }
        }

        report.duration_ms = start.elapsed().as_millis() as u64;
        Ok(report)
    }

    pub fn status(&self) -> Result<StoreReport, StoreError> {
        let total_packages = self.index.package_count()?;
        let total_projects = self.index.project_count()?;
        let total_size_bytes = self.index.total_size()?;

        let unreferenced = self.index.get_unreferenced_packages()?;
        let mut unreferenced_packages = Vec::new();
        let mut reclaimable_bytes = 0;
        for pkg in &unreferenced {
            unreferenced_packages.push(pkg.integrity.clone());
            reclaimable_bytes += pkg.size_bytes;
        }

        Ok(StoreReport {
            total_packages,
            total_projects,
            total_size_bytes,
            unreferenced_packages,
            reclaimable_bytes,
            ..Default::default()
        })
    }
pub fn prune(&self, dry_run: bool) -> Result<StoreReport, StoreError> {
        if !dry_run && self.index.is_readonly() {
            return Err(StoreError::Database(
                "cannot prune: store index is readonly".into(),
            ));
        }

        let unreferenced = self.index.get_unreferenced_packages()?;
        let mut unreferenced_packages = Vec::new();
        let mut reclaimable_bytes = 0;

        for pkg in &unreferenced {
            unreferenced_packages.push(pkg.integrity.clone());
            reclaimable_bytes += pkg.size_bytes;

            if !dry_run {
                let cas_path = self
                    .store
                    .root()
                    .join(&pkg.shard)
                    .join(&pkg.filename);

                if cas_path.exists() {
                    let meta = fs::metadata(&cas_path).map_err(|e| StoreError::Io {
                        path: cas_path.clone(),
                        msg: e.to_string(),
                    })?;
                    if meta.file_type().is_symlink() {
                        tracing::warn!("skipping symlink during prune: {}", cas_path.display());
                    } else {
                        fs::remove_file(&cas_path).map_err(|e| StoreError::Io {
                            path: cas_path.clone(),
                            msg: e.to_string(),
                        })?;
                    }
                }

                self.index.delete_package(&pkg.integrity)?;
            }
        }

        if !dry_run {
            self.cleanup_empty_shards()?;
        }

        Ok(StoreReport {
            unreferenced_packages,
            reclaimable_bytes,
            ..Default::default()
        })
    }

    fn cleanup_empty_shards(&self) -> Result<(), StoreError> {
        let cas_root = self.store.root();
        if !cas_root.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(cas_root).map_err(|e| StoreError::Io {
            path: cas_root.to_path_buf(),
            msg: e.to_string(),
        })? {
            let entry = entry.map_err(|e| StoreError::Io {
                path: cas_root.to_path_buf(),
                msg: e.to_string(),
            })?;
            let path = entry.path();
            if path.is_dir() {
                let meta = fs::metadata(&path).map_err(|e| StoreError::Io {
                    path: path.clone(),
                    msg: e.to_string(),
                })?;
                if meta.file_type().is_symlink() {
                    continue; // Skip symlinks
                }
                let mut dir_iter = fs::read_dir(&path).map_err(|e| StoreError::Io {
                    path: path.clone(),
                    msg: e.to_string(),
                })?;
                if dir_iter.next().is_none() {
                    fs::remove_dir(&path).map_err(|e| StoreError::Io {
                        path: path.clone(),
                        msg: e.to_string(),
                    })?;
                }
            }
        }

        Ok(())
    }
}
