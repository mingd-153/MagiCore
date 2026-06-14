use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use sha2::{Digest, Sha256};
use tokio::fs;

/// Simple content‑addressable cache stored under $HOME/.core-pkg/cache
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let root = PathBuf::from(home).join(".core-pkg/cache");
        std::fs::create_dir_all(&root).ok();
        Cache { root }
    }

    /// Resolve a set of URLs (or tarball paths) and store them if not present.
    /// For now this is a stub – real implementation would download & hash.
    pub async fn resolve(&self, _items: &Vec<String>) -> Result<()> {
        // placeholder: in a real version we would fetch each URL, compute sha256,
        // and store under root/<sha256>.tar.gz.
        Ok(())
    }

    fn hash_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }
}
