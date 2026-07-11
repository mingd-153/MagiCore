use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntegrityHash {
    pub hash: String,
    pub executable: bool,
}

impl IntegrityHash {
    pub fn from_bytes(data: &[u8], executable: bool) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        Self {
            hash: hex::encode(hasher.finalize()),
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
        let algo_dir = root.join("files").join("sha256");
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
            "sha256-{}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_integrity_hash_consistency() {
        let h1 = IntegrityHash::from_bytes(b"hello world", false);
        let h2 = IntegrityHash::from_bytes(b"hello world", false);
        assert_eq!(h1, h2);
        assert_eq!(h1.hash.len(), 64);
    }

    #[test]
    fn test_different_content_different_hash() {
        let h1 = IntegrityHash::from_bytes(b"hello", false);
        let h2 = IntegrityHash::from_bytes(b"world", false);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_cas_path_creates_correct_structure() {
        let root = tempdir().unwrap();
        let hash = IntegrityHash::from_bytes(b"test", false);
        let path = hash.cas_path(root.path());
        assert!(path.to_string_lossy().contains("files/sha256/"));
        assert!(path.to_string_lossy().contains(&hash.hash[..2]));
        assert!(path.to_string_lossy().ends_with(&hash.hash));
    }

    #[test]
    fn test_cas_path_executable() {
        let root = tempdir().unwrap();
        let hash = IntegrityHash::from_bytes(b"#!/bin/bash", true);
        let path = hash.cas_path(root.path());
        assert_eq!(path.extension().unwrap(), "exec");
    }
}
