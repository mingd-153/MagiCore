use super::integrity::IntegrityHash;
use super::store::StoreError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledModule {
    pub js: String,
    pub source_map: Option<String>,
}

pub struct CompiledCache {
    root: PathBuf,
}

impl CompiledCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn module_path(&self, source_hash: &IntegrityHash) -> PathBuf {
        let algo_dir = self.root.join("compiled").join("blake3");
        let first2 = &source_hash.hash[..2];
        algo_dir.join(first2).join(&source_hash.hash).with_extension("json")
    }

    pub fn get(&self, source_hash: &IntegrityHash) -> Result<Option<CompiledModule>, StoreError> {
        let path = self.module_path(source_hash);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)?;
        let module: CompiledModule = serde_json::from_slice(&data).map_err(|e| StoreError::Io {
            path: path.clone(),
            msg: format!("failed to parse compiled module: {}", e),
        })?;
        Ok(Some(module))
    }

    pub fn put(&self, source_hash: &IntegrityHash, module: &CompiledModule) -> Result<(), StoreError> {
        let path = self.module_path(source_hash);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let tmp = path.with_extension("tmp");
        let data = serde_json::to_vec(module).map_err(|e| StoreError::Io {
            path: tmp.clone(),
            msg: format!("failed to serialize compiled module: {}", e),
        })?;
        
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path)?;
        
        Ok(())
    }
}
