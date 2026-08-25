/// Store directory layout and conventions.
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// CAS files directory (content-addressed blob storage)
    pub fn cas_dir(&self) -> PathBuf {
        self.root.join("cas")
    }

    /// Package tarball cache directory
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// SQLite database path
    pub fn db_path(&self) -> PathBuf {
        self.root.join("store.db")
    }

    /// Temporary directory for incomplete downloads
    pub fn temp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Logs directory
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Lockfile directory
    pub fn locks_dir(&self) -> PathBuf {
        self.root.join("locks")
    }

    /// Virtual store directory (pnpm style)
    pub fn virtual_store_dir(&self) -> PathBuf {
        self.root.join("virtual_store")
    }

    /// MessagePack package-file index (fast layer, rebuildable from SQLite).
    pub fn index_msgpack_path(&self) -> PathBuf {
        self.root.join("index.msgpack")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_layout_paths() {
        let root = tempdir().unwrap();
        let layout = Layout::new(root.path().to_path_buf());
        assert!(layout.cas_dir().to_string_lossy().contains("cas"));
        assert!(layout.cache_dir().to_string_lossy().contains("cache"));
        assert!(layout.db_path().to_string_lossy().ends_with("store.db"));
    }
}
