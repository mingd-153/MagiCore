/// Package cache for downloaded tarballs and metadata.
use anyhow::Result;
use mgc_types::PackageId;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

struct TempPathGuard {
    path: PathBuf,
    armed: bool,
}

impl TempPathGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempPathGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cache-entry");
    parent.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

fn create_unique_temp_file(path: &Path) -> Result<(PathBuf, File)> {
    for _ in 0..16 {
        let temp = unique_temp_path(path);
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    anyhow::bail!(
        "could not allocate a unique cache staging file for {}",
        path.display()
    )
}

fn same_file_contents(left: &Path, right: &Path) -> Result<bool> {
    let left_meta = std::fs::symlink_metadata(left)?;
    let right_meta = std::fs::symlink_metadata(right)?;
    if !left_meta.file_type().is_file() || !right_meta.file_type().is_file() {
        return Ok(false);
    }
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }

    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut left_hash = blake3::Hasher::new();
    let mut right_hash = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = left.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        left_hash.update(&buffer[..read]);
    }
    loop {
        let read = right.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        right_hash.update(&buffer[..read]);
    }
    Ok(left_hash.finalize() == right_hash.finalize())
}

fn publish_cache_entry(temp: &Path, path: &Path) -> Result<()> {
    match std::fs::hard_link(temp, path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            if !same_file_contents(temp, path)? {
                anyhow::bail!(
                    "cache conflict at {}: concurrent writers produced different content",
                    path.display()
                );
            }
        }
        Err(err) => return Err(err.into()),
    }
    std::fs::remove_file(temp)?;
    Ok(())
}

#[derive(Clone)]
pub struct PackageCache {
    root: PathBuf,
}

impl PackageCache {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Flatten an ecosystem identifier into ONE safe path segment.
    /// Multi-language names (Maven `group:artifact`, OSV SwiftURL
    /// `github.com/owner/repo`) carry separators that would nest or
    /// reshape the cache tree — collapse them to `_` so every package
    /// lives at depth 1 (no traversal, no aliasing via nesting).
    /// Sanitize định danh ecosystem thành MỘT segment path an toàn:
    /// tên đa ngôn ngữ (Maven, SwiftURL) mang dấu phân cách sẽ làm
    /// tổ láp cây cache — ép thành `_` để mọi package nằm ở độ sâu 1.
    fn safe_segment(name: &str) -> String {
        name.replace(['/', '\\', ':'], "_")
    }

    /// Path to cached tarball for a given package version
    pub fn tarball_path(&self, id: &PackageId) -> PathBuf {
        self.root
            .join(Self::safe_segment(id.name_str()))
            .join(format!("{}.tgz", id.version()))
    }

    /// Path to cached metadata JSON for a package
    pub fn metadata_path(&self, name: &str) -> PathBuf {
        self.root
            .join(Self::safe_segment(name))
            .join("metadata.json")
    }

    /// Check if a package version is cached
    pub fn contains_tarball(&self, id: &PackageId) -> bool {
        self.tarball_path(id).exists()
    }

    /// Check if package metadata is cached
    pub fn contains_metadata(&self, name: &str) -> bool {
        self.metadata_path(name).exists()
    }

    /// Cache a tarball to disk
    pub fn cache_tarball(&self, id: &PackageId, data: &[u8]) -> Result<()> {
        let path = self.tarball_path(id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let (tmp, mut file) = create_unique_temp_file(&path)?;
        let guard = TempPathGuard::new(tmp);
        file.write_all(data)?;
        drop(file);
        publish_cache_entry(guard.path(), &path)
    }

    /// Cache a tarball by linking/copying an existing tarball path.
    pub fn cache_tarball_from_path(&self, id: &PackageId, source: &std::path::Path) -> Result<()> {
        let path = self.tarball_path(id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        for _ in 0..16 {
            let tmp = unique_temp_path(&path);
            match std::fs::hard_link(source, &tmp) {
                Ok(()) => {
                    let guard = TempPathGuard::new(tmp);
                    return publish_cache_entry(guard.path(), &path);
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    let mut destination =
                        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
                            Ok(file) => file,
                            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                            Err(err) => return Err(err.into()),
                        };
                    let guard = TempPathGuard::new(tmp);
                    let mut source_file = File::open(source)?;
                    std::io::copy(&mut source_file, &mut destination)?;
                    drop(destination);
                    return publish_cache_entry(guard.path(), &path);
                }
            }
        }
        anyhow::bail!(
            "could not allocate a unique cache staging file for {}",
            path.display()
        )
    }

    /// Cache metadata JSON
    pub fn cache_metadata(&self, name: &str, data: &[u8]) -> Result<()> {
        let path = self.metadata_path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let (tmp, mut file) = create_unique_temp_file(&path)?;
        let mut guard = TempPathGuard::new(tmp);
        file.write_all(data)?;
        drop(file);
        // Metadata is mutable (registry refreshes), so publish atomically with
        // last-writer-wins semantics but never share a staging name.
        std::fs::rename(guard.path(), path)?;
        guard.disarm();
        Ok(())
    }

    /// Read cached tarball
    pub fn get_tarball(&self, id: &PackageId) -> Result<Option<Vec<u8>>> {
        let path = self.tarball_path(id);
        if path.exists() {
            Ok(Some(std::fs::read(path)?))
        } else {
            Ok(None)
        }
    }

    /// Clear all cached packages
    pub fn clear(&self) -> Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
            std::fs::create_dir_all(&self.root)?;
        }
        Ok(())
    }

    /// Total number of cached tarballs
    pub fn tarball_count(&self) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let pkg_dir = entry.path();
                if pkg_dir.is_dir() {
                    count += std::fs::read_dir(&pkg_dir)
                        .map(|e| {
                            e.filter_map(|e| e.ok())
                                .filter(|e| e.path().extension().unwrap_or_default() == "tgz")
                                .count()
                        })
                        .unwrap_or(0);
                }
            }
        }
        count
    }

    /// Total disk usage in bytes
    pub fn disk_usage(&self) -> u64 {
        let mut total = 0u64;
        let entries = walkdir::WalkDir::new(&self.root).into_iter();
        for entry in entries.flatten() {
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if entry.file_type().is_file()
                && let Ok(meta) = entry.metadata()
            {
                total += meta.len();
            }
        }
        total
    }
}
