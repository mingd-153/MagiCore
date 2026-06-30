use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use super::index::{ProjectInfo, StoreError, StoreIndex};

const GVS_VERSION: &str = "v1";
const DEP_GRAPH_HASH_LEN: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GvsStats {
    pub total_projects: usize,
    pub total_packages: usize,
    pub total_symlinks: usize,
    pub total_size_bytes: u64,
    pub gvs_root: PathBuf,
    pub reclaimable_dirs: usize,
    pub reclaimable_symlinks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GvsGcReport {
    pub removed_dirs: Vec<PathBuf>,
    pub removed_symlinks: usize,
    pub reclaimed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GvsMetadata {
    version: String,
    dep_graph_hash: String,
    created_at: u64,
}

pub struct GlobalVirtualStore {
    gvs_root: PathBuf,
}

impl GlobalVirtualStore {
    pub fn new(gvs_root: PathBuf) -> Self {
        Self { gvs_root }
    }

    pub fn root(&self) -> &Path {
        &self.gvs_root
    }

    pub fn ensure_dirs(&self) -> Result<(), StoreError> {
        fs::create_dir_all(&self.gvs_root).map_err(|e| StoreError::Io {
            path: self.gvs_root.clone(),
            msg: e.to_string(),
        })
    }

    fn validate_dep_graph_hash(hash: &str) -> Result<(), StoreError> {
        if hash.len() != DEP_GRAPH_HASH_LEN {
            return Err(StoreError::Database(format!(
                "invalid dep_graph_hash length: expected {}, got {}",
                DEP_GRAPH_HASH_LEN,
                hash.len()
            )));
        }
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(StoreError::Database(format!(
                "dep_graph_hash must be lowercase hex, got: {}",
                hash
            )));
        }
        Ok(())
    }

    fn acquire_lock(&self) -> Result<fs::File, StoreError> {
        let lock_path = self.gvs_root.join(".gvs.lock");
        fs::create_dir_all(&self.gvs_root).map_err(|e| StoreError::Io {
            path: self.gvs_root.clone(),
            msg: e.to_string(),
        })?;
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| StoreError::Io {
                path: lock_path,
                msg: e.to_string(),
            })?;
        lock_file
            .try_lock_exclusive()
            .map_err(|e| StoreError::Database(format!("failed to acquire GVS lock: {}", e)))?;
        Ok(lock_file)
    }

    pub fn register(
        &self,
        project_path: &Path,
        dep_graph_hash: &str,
        index: &dyn StoreIndex,
    ) -> Result<(), StoreError> {
        Self::validate_dep_graph_hash(dep_graph_hash)?;

        let _lock = self.acquire_lock()?;

        let canonical_path = fs::canonicalize(project_path).map_err(|e| StoreError::Io {
            path: project_path.to_path_buf(),
            msg: format!("failed to canonicalize project path: {}", e),
        })?;

        index.register_project(&canonical_path)?;

        let mut meta = serde_json::Map::new();
        meta.insert(
            "dep_graph_hash".to_string(),
            serde_json::Value::String(dep_graph_hash.to_string()),
        );
        let meta_json =
            serde_json::to_string(&meta).map_err(|e| StoreError::Serialization(e.to_string()))?;
        index.set_project_metadata(&canonical_path, &meta_json)?;

        let gvs_dir = self.gvs_dir_for(dep_graph_hash);
        if gvs_dir.exists() {
            self.verify_metadata(&gvs_dir, dep_graph_hash)?;
            return Ok(());
        }

        let node_modules = gvs_dir.join("node_modules");
        fs::create_dir_all(&node_modules).map_err(|e| StoreError::Io {
            path: node_modules.clone(),
            msg: e.to_string(),
        })?;

        let meta = GvsMetadata {
            version: GVS_VERSION.to_string(),
            dep_graph_hash: dep_graph_hash.to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        let meta_path = gvs_dir.join(".mgpm-gvs.json");
        let meta_json =
            serde_json::to_string_pretty(&meta).map_err(|e| StoreError::Serialization(e.to_string()))?;
        fs::write(&meta_path, &meta_json).map_err(|e| StoreError::Io {
            path: meta_path,
            msg: e.to_string(),
        })?;

        self.verify_metadata(&gvs_dir, dep_graph_hash)?;
        Ok(())
    }

    fn verify_metadata(&self, gvs_dir: &Path, expected_hash: &str) -> Result<(), StoreError> {
        let meta_path = gvs_dir.join(".mgpm-gvs.json");
        if !meta_path.exists() {
            return Err(StoreError::Database("GVS metadata file missing".into()));
        }
        let content = fs::read_to_string(&meta_path).map_err(|e| StoreError::Io {
            path: meta_path,
            msg: e.to_string(),
        })?;
        let meta: GvsMetadata = serde_json::from_str(&content)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        if meta.dep_graph_hash != expected_hash {
            return Err(StoreError::Database(format!(
                "metadata hash mismatch: expected {}, got {}",
                expected_hash, meta.dep_graph_hash
            )));
        }
        if meta.version != GVS_VERSION {
            return Err(StoreError::Database(format!(
                "unsupported GVS version: {} (expected {})",
                meta.version, GVS_VERSION
            )));
        }
        Ok(())
    }

    pub fn unregister(
        &self,
        project_path: &Path,
        index: &dyn StoreIndex,
    ) -> Result<(), StoreError> {
        let _lock = self.acquire_lock()?;

        let canonical_path = fs::canonicalize(project_path).map_err(|e| StoreError::Io {
            path: project_path.to_path_buf(),
            msg: format!("failed to canonicalize project path: {}", e),
        })?;
        let path_str = canonical_path.to_string_lossy().to_string();

        let projects = index.list_projects()?;
        let dep_graph_hash = projects
            .iter()
            .find(|p| p.path == path_str)
            .and_then(|p| p.dep_graph_hash());

        index.unregister_project(&canonical_path)?;

        if let Some(ref hash) = dep_graph_hash {
            let fresh_projects = index.list_projects()?;
            let remaining = fresh_projects
                .iter()
                .filter(|p| p.path != path_str && p.dep_graph_hash() == Some(hash.clone()))
                .count();

            if remaining == 0 {
                self.remove_gvs_dir(hash);
            }
        }

        Ok(())
    }

    pub fn list_projects(&self, index: &dyn StoreIndex) -> Result<Vec<ProjectInfo>, StoreError> {
        index.list_projects()
    }

    pub fn status(&self, index: &dyn StoreIndex) -> Result<GvsStats, StoreError> {
        let projects = index.list_projects()?;
        let total_projects = projects.len();

        let active_hashes: HashSet<String> = projects
            .iter()
            .filter_map(|p| p.dep_graph_hash())
            .collect();

        let total_packages = index.package_count().unwrap_or(0) as usize;
        let total_size = index.total_size().unwrap_or(0);

        let mut total_symlinks = 0;
        let mut reclaimable_symlinks = 0;
        let mut reclaimable_dirs = 0;

        if self.gvs_root.exists() {
            if let Ok(entries) = fs::read_dir(&self.gvs_root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let dir_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    let nm = path.join("node_modules");
                    let count = if nm.exists() {
                        fs::read_dir(&nm).map(|e| e.flatten().count()).unwrap_or(0)
                    } else {
                        0
                    };
                    total_symlinks += count;

                    if !active_hashes.contains(&dir_name) {
                        reclaimable_symlinks += count;
                        reclaimable_dirs += 1;
                    }
                }
            }
        }

        Ok(GvsStats {
            total_projects,
            total_packages,
            total_symlinks,
            total_size_bytes: total_size,
            gvs_root: self.gvs_root.clone(),
            reclaimable_dirs,
            reclaimable_symlinks,
        })
    }

    pub fn gc(&self, index: &dyn StoreIndex) -> Result<GvsGcReport, StoreError> {
        let _lock = self.acquire_lock()?;

        let projects = index.list_projects()?;
        let active_hashes: HashSet<String> = projects
            .iter()
            .filter_map(|p| p.dep_graph_hash())
            .collect();

        let mut removed_dirs = Vec::new();
        let mut removed_symlinks = 0;
        let mut reclaimed_bytes = 0;

        if self.gvs_root.exists() {
            if let Ok(entries) = fs::read_dir(&self.gvs_root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let dir_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    if !active_hashes.contains(&dir_name) {
                        let project_still_exists = projects.iter().any(|p| {
                            p.dep_graph_hash() == Some(dir_name.clone())
                                && Path::new(&p.path).exists()
                        });
                        if project_still_exists {
                            continue;
                        }

                        let nm = path.join("node_modules");
                        if nm.exists() {
                            if let Ok(nm_entries) = fs::read_dir(&nm) {
                                for nm_entry in nm_entries.flatten() {
                                    if let Ok(meta) = nm_entry.path().symlink_metadata() {
                                        reclaimed_bytes += meta.len();
                                    }
                                    removed_symlinks += 1;
                                }
                            }
                            fs::remove_dir_all(&nm).ok();
                        }

                        let meta_path = path.join(".mgpm-gvs.json");
                        if meta_path.exists() {
                            fs::remove_file(&meta_path).ok();
                        }

                        fs::remove_dir(&path).ok();
                        removed_dirs.push(path);
                    }
                }
            }
        }

        Ok(GvsGcReport {
            removed_dirs,
            removed_symlinks,
            reclaimed_bytes,
        })
    }

    fn gvs_dir_for(&self, dep_graph_hash: &str) -> PathBuf {
        self.gvs_root.join(dep_graph_hash)
    }

    fn remove_gvs_dir(&self, dep_graph_hash: &str) {
        let gvs_dir = self.gvs_dir_for(dep_graph_hash);
        if !gvs_dir.exists() {
            return;
        }

        let nm = gvs_dir.join("node_modules");
        if nm.exists() {
            fs::remove_dir_all(&nm).ok();
        }

        let meta_path = gvs_dir.join(".mgpm-gvs.json");
        if meta_path.exists() {
            fs::remove_file(&meta_path).ok();
        }

        fs::remove_dir(&gvs_dir).ok();
    }
}
