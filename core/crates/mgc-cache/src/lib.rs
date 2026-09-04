//! MagiCore Package Cache
//! Cache quản lý packages với integrity verification

pub mod memory;

use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Package cache — Cache packages
pub struct PackageCache {
    root: PathBuf,
    /// Per-package locks for concurrent access safety — Lock per-package cho an toàn concurrent
    locks: DashMap<String, Arc<RwLock<()>>>,
}

/// Cache entry metadata — Metadata entry cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub package_id: String,
    pub integrity: String,
    pub cached_at: String,
    pub size_bytes: u64,
}

impl PackageCache {
    /// Create new cache at default location — Tạo cache mới ở vị trí mặc định
    pub fn new() -> Result<Self> {
        let root = Self::default_cache_dir()?;
        fs::create_dir_all(&root)?;

        // O2 FIX (AUDIT V2): Set strict permissions (owner-only) on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&root)?.permissions();
            perms.set_mode(0o700); // rwx------ (owner-only)
            fs::set_permissions(&root, perms)?;
        }

        Ok(Self {
            root,
            locks: DashMap::new(), // O1: Per-package locks
        })
    }

    /// Get default cache directory — Lấy thư mục cache mặc định
    pub fn default_cache_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("No home directory")?;
        Ok(home.join(".magicore").join("cache"))
    }

    /// Check if package cached — Kiểm tra package có trong cache không
    pub fn has_package(&self, package_id: &str) -> bool {
        self.package_path(package_id).exists()
    }

    /// Get package from cache (with integrity check) — Lấy package từ cache
    pub fn get_package(&self, package_id: &str, expected_integrity: &str) -> Result<PathBuf> {
        // O1: Acquire read lock (concurrent reads OK, blocks writes)
        let lock = self
            .locks
            .entry(package_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())));
        let _guard = lock.value().read().unwrap();

        let pkg_path = self.package_path(package_id);
        if !pkg_path.exists() {
            anyhow::bail!("Package not in cache: {}", package_id);
        }

        // T4.2: Verify integrity
        let actual_integrity = self.compute_integrity(&pkg_path)?;
        if actual_integrity != expected_integrity {
            anyhow::bail!(
                "Integrity mismatch for {}: expected {}, got {}",
                package_id,
                expected_integrity,
                actual_integrity
            );
        }

        Ok(pkg_path)
    }

    /// Store package to cache — Lưu package vào cache
    pub fn store_package(
        &self,
        package_id: &str,
        source: &Path,
        integrity: &str,
    ) -> Result<PathBuf> {
        // O1: Acquire write lock (exclusive access)
        let lock = self
            .locks
            .entry(package_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())));
        let _guard = lock.value().write().unwrap();

        let pkg_dir = self.package_dir(package_id);
        fs::create_dir_all(&pkg_dir)?;

        // O1 FIX (AUDIT V2): Atomic write to prevent race condition
        let temp_path = pkg_dir.join(".package.tgz.tmp");
        let pkg_path = pkg_dir.join("package.tgz");

        // Write to temp file
        fs::copy(source, &temp_path)?;

        // Verify integrity BEFORE commit (prevent poisoning)
        let actual_integrity = self.compute_integrity(&temp_path)?;
        if actual_integrity != integrity {
            fs::remove_file(&temp_path)?;
            anyhow::bail!(
                "Integrity mismatch during store for {}: expected {}, got {}",
                package_id,
                integrity,
                actual_integrity
            );
        }

        // Atomic rename (overwrites existing if present)
        fs::rename(&temp_path, &pkg_path)?;

        // O3 OPTIMIZATION (AUDIT V2): Remove .integrity file (redundant)
        // Lockfile already stores integrity hash; no need to duplicate on disk
        // Keeps cache footprint smaller

        Ok(pkg_path)
    }

    /// Invalidate package (T4.5: tamper detection) — Invalidate package
    pub fn invalidate_package(&self, package_id: &str) -> Result<()> {
        let pkg_dir = self.package_dir(package_id);
        if pkg_dir.exists() {
            fs::remove_dir_all(&pkg_dir)?;
        }
        Ok(())
    }

    /// Prune unused packages — Xóa packages không dùng
    pub fn prune(&self) -> Result<usize> {
        // Simple implementation: remove all (full prune)
        // Issue #11: Smart prune (check lockfiles before deletion)
        let packages_dir = self.root.join("packages");
        if !packages_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        for entry in fs::read_dir(&packages_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                fs::remove_dir_all(entry.path())?;
                count += 1;
            }
        }
        Ok(count)
    }

    // Internal helpers

    fn package_dir(&self, package_id: &str) -> PathBuf {
        // Sanitize package_id for filesystem
        let sanitized = package_id.replace(['/', '\\', ':'], "_");
        self.root.join("packages").join("npm").join(sanitized)
    }

    /// Get package path (public for testing) — Lấy đường dẫn package
    pub fn package_path(&self, package_id: &str) -> PathBuf {
        self.package_dir(package_id).join("package.tgz")
    }

    /// Compute integrity of a file (public for testing) — Tính integrity file
    pub fn compute_integrity(&self, path: &Path) -> Result<String> {
        // T5.3: Use streaming BLAKE3 (zero-copy for large files)
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(path)?;
        let mut hasher = blake3::Hasher::new();

        // Stream in 64KB chunks (optimal for BLAKE3 SIMD)
        let mut buffer = vec![0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        let hash = hasher.finalize();
        Ok(format!("blake3:{}", hash.to_hex()))
    }

    /// Create cache with custom root (for testing) — Tạo cache với root tùy chỉnh
    #[doc(hidden)]
    pub fn with_root(root: PathBuf) -> Self {
        Self {
            root,
            locks: DashMap::new(),
        }
    }

    /// T5.2: Get multiple packages in parallel (rayon) — Lấy nhiều packages song song
    pub fn get_packages_parallel(
        &self,
        requests: &[(String, String)], // (package_id, expected_integrity)
    ) -> Result<Vec<PathBuf>> {
        use rayon::prelude::*;

        requests
            .par_iter()
            .map(|(pkg_id, integrity)| self.get_package(pkg_id, integrity))
            .collect()
    }

    /// T5.2: Store multiple packages in parallel — Lưu nhiều packages song song
    pub fn store_packages_parallel(
        &self,
        requests: &[(String, PathBuf, String)], // (package_id, source, integrity)
    ) -> Result<Vec<PathBuf>> {
        use rayon::prelude::*;

        requests
            .par_iter()
            .map(|(pkg_id, source, integrity)| self.store_package(pkg_id, source, integrity))
            .collect()
    }
}

impl Default for PackageCache {
    fn default() -> Self {
        Self::new().expect("Failed to create default cache")
    }
}
