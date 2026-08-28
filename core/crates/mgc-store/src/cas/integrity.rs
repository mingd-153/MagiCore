use blake3::Hasher;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntegrityHash {
    pub hash: String,
    pub executable: bool,
}

impl IntegrityHash {
    pub fn from_bytes(data: &[u8], executable: bool) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(data);
        Self {
            hash: hasher.finalize().to_hex().to_string(),
            executable,
        }
    }

    pub fn from_hash_str(hash_hex: &str, executable: bool) -> Self {
        Self {
            hash: hash_hex.to_string(),
            executable,
        }
    }

    pub fn cas_path(&self, root: &Path) -> PathBuf {
        let algo_dir = root.join("files").join("blake3");
        let first2 = &self.hash[..2];
        let mut path = algo_dir.join(first2).join(&self.hash);
        if self.executable {
            path.set_extension("exec");
        }
        path
    }

    pub fn to_integrity_str(&self) -> String {
        use base64::Engine;
        let raw = hex::decode(&self.hash).unwrap_or_default();
        format!(
            "blake3-{}",
            base64::engine::general_purpose::STANDARD.encode(&raw)
        )
    }
}

#[derive(Debug, Clone)]
pub struct TarballEntry {
    pub path: String,
    pub data: Vec<u8>,
    pub executable: bool,
}

