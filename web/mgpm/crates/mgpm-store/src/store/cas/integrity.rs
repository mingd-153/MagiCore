use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IntegrityHash {
    pub hash: String,
    pub shard: String,
    pub filename: String,
    pub is_executable: bool,
}

impl IntegrityHash {
    pub fn from_bytes(data: &[u8], executable: bool) -> Self {
        let hash = hex::encode(Sha256::digest(data));
        let shard = hash[..2].to_string();
        let filename = if executable {
            format!("{}-exec", hash)
        } else {
            hash.clone()
        };
        Self {
            hash,
            shard,
            filename,
            is_executable: executable,
        }
    }

    pub fn cas_path(&self, cas_root: &Path) -> PathBuf {
        cas_root.join(&self.shard).join(&self.filename)
    }
}

pub struct TarballEntry {
    pub path: String,
    pub data: Vec<u8>,
    pub executable: bool,
}
