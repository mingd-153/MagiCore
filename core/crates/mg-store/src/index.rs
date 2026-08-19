//! Store package-file index (T6).
//!
//! Fast layer: single msgpack file mapping each indexed package to its file
//! listing (path + blob hash + size). Source of truth: SQLite `package_files`
//! table (`Database::replace_package_files`). If the msgpack file is missing
//! or its checksum mismatches it is rebuilt from SQLite — never trusted blind.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::layout::Layout;
use mg_types::PackageId;

const INDEX_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub blob_hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageIndex {
    id: String,
    version: String,
    files: Vec<FileEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexFile {
    version: u32,
    checksum: String,
    packages: Vec<PackageIndex>,
}

pub struct StoreIndex {
    db: Database,
    msgpack_path: std::path::PathBuf,
    /// id#version -> files (mirrors SQLite, loaded from msgpack at open).
    packages: HashMap<String, Vec<FileEntry>>,
}

fn package_key(id: &PackageId) -> String {
    format!("{}#{}", id.name_str(), id.version())
}

impl StoreIndex {
    pub fn open(layout: &Layout, db: Database) -> Result<Self> {
        let msgpack_path = layout.index_msgpack_path();
        let mut index = Self {
            db,
            msgpack_path,
            packages: HashMap::new(),
        };
        index.load_or_rebuild()?;
        Ok(index)
    }

    fn load_or_rebuild(&mut self) -> Result<()> {
        if self.msgpack_path.exists() {
            let raw = std::fs::read(&self.msgpack_path)
                .with_context(|| format!("read index msgpack {}", self.msgpack_path.display()))?;
            match rmp_serde::from_slice::<IndexFile>(&raw) {
                Ok(file) if file.version == INDEX_FORMAT_VERSION => {
                    let checksum = checksum_of_packages(&file.packages);
                    if checksum == file.checksum {
                        self.packages = packages_to_map(&file.packages);
                        return Ok(());
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }
        // Missing / corrupt / stale version -> rebuild from SQLite.
        self.rebuild()
    }

    /// Rebuild the msgpack file from SQLite (source of truth).
    pub fn rebuild(&mut self) -> Result<()> {
        let mut stmt = self
            .db
            .conn()
            .prepare(
                "SELECT id, version, path, blob_hash, size FROM package_files ORDER BY id, version, path",
            )
            .context("prepare package_files scan")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u64>(4)?,
                ))
            })
            .context("scan package_files")?;

        let mut order: Vec<(String, String)> = Vec::new();
        let mut by_pkg: HashMap<String, Vec<FileEntry>> = HashMap::new();
        for row in rows {
            let (id, version, path, blob_hash, size) = row?;
            let key = format!("{id}#{version}");
            if !by_pkg.contains_key(&key) {
                order.push((id, version));
            }
            by_pkg.entry(key).or_default().push(FileEntry {
                path,
                blob_hash,
                size,
            });
        }

        let packages: Vec<PackageIndex> = order
            .into_iter()
            .map(|(id, version)| {
                let key = format!("{id}#{version}");
                PackageIndex {
                    id,
                    version,
                    files: by_pkg.remove(&key).unwrap_or_default(),
                }
            })
            .collect();
        self.packages = packages_to_map(&packages);
        self.persist(&packages)
    }

    fn persist(&self, packages: &[PackageIndex]) -> Result<()> {
        let file = IndexFile {
            version: INDEX_FORMAT_VERSION,
            checksum: checksum_of_packages(packages),
            packages: packages.to_vec(),
        };
        let data = rmp_serde::to_vec(&file).context("encode index msgpack")?;
        atomic_write(&self.msgpack_path, &data)
    }

    /// Upsert one package's file listing (SQLite + msgpack in lockstep).
    pub fn upsert_package_files(&mut self, id: &PackageId, files: Vec<FileEntry>) -> Result<()> {
        let sql_files: Vec<(String, String, u64)> = files
            .iter()
            .map(|f| (f.path.clone(), f.blob_hash.clone(), f.size))
            .collect();
        self.db.replace_package_files(id, &sql_files)?;
        self.packages.insert(package_key(id), files);

        // Persist full msgpack from current map (keeps other packages intact).
        let mut packages: Vec<PackageIndex> = self
            .packages
            .iter()
            .map(|(key, files)| {
                let (id, version) = key.split_once('#').unwrap_or((key.as_str(), ""));
                PackageIndex {
                    id: id.to_string(),
                    version: version.to_string(),
                    files: files.clone(),
                }
            })
            .collect();
        packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
        self.persist(&packages)
    }

    /// Full file listing for a package — None = not indexed.
    pub fn verify_package(&self, id: &PackageId) -> Option<&[FileEntry]> {
        self.packages.get(&package_key(id)).map(Vec::as_slice)
    }

    /// Blob hashes currently referenced by any indexed package (from SQLite —
    /// source of truth, even if msgpack is stale).
    pub fn list_blob_hashes(&self) -> Result<Vec<String>> {
        self.db.list_all_blob_hashes()
    }

    /// Prune CAS blobs not referenced by any indexed package.
    ///
    /// Safe-guards (fail-closed): only files older than `max_age` are removed,
    /// and only when (a) the path lives under the CAS blob layout AND (b) the
    /// blob hash is unknown to the index AND (c) the file has no external
    /// hardlinks (nlink <= 1). If the index holds no data at all while blobs
    /// exist (index not yet built), this returns an error instead of deleting.
    pub fn prune_blobs(&self, cas_root: &Path, max_age: std::time::Duration) -> Result<usize> {
        if !cas_root.exists() {
            return Ok(0);
        }
        let live: std::collections::HashSet<String> =
            self.list_blob_hashes()?.into_iter().collect();
        let indexed_count = self.db.count_indexed_packages()?;
        if indexed_count == 0 && !self.packages.is_empty() {
            anyhow::bail!(
                "index loaded from msgpack but SQLite is empty — aborting prune, run rebuild()"
            );
        }
        if live.is_empty() {
            return Ok(0);
        }

        let mut removed = 0usize;
        let blobs_root = cas_root.join("files").join("blake3");
        if !blobs_root.exists() {
            return Ok(0);
        }
        for entry in walkdir::WalkDir::new(&blobs_root)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() || !path_is_older_than(entry.path(), max_age) {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let hash = name.trim_end_matches(".exec").to_string();
            if live.contains(&hash) {
                continue;
            }
            if !file_has_no_external_hardlinks(entry.path()) {
                continue;
            }
            if std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn path_is_older_than(path: &Path, max_age: std::time::Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().map(|age| age > max_age).unwrap_or(false))
        .unwrap_or(false)
}

fn file_has_no_external_hardlinks(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.nlink() <= 1)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn checksum_of_packages(packages: &[PackageIndex]) -> String {
    let mut hasher = Hasher::new();
    for p in packages {
        hasher.update(p.id.as_bytes());
        hasher.update(p.version.as_bytes());
        for f in &p.files {
            hasher.update(f.path.as_bytes());
            hasher.update(f.blob_hash.as_bytes());
            hasher.update(&f.size.to_le_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn packages_to_map(packages: &[PackageIndex]) -> HashMap<String, Vec<FileEntry>> {
    packages
        .iter()
        .map(|p| (format!("{}#{}", p.id, p.version), p.files.clone()))
        .collect()
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("msgpack.tmp");
    std::fs::write(&tmp, data).with_context(|| format!("write tmp {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}
