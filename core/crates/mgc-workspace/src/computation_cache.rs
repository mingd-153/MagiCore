use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

/// Metadata for each package's workspace computation cache.
/// Metadata lưu trữ cache computation của từng package trong workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageBuildCache {
    /// Combined hash of source code and local configuration.
    /// Hash tổng hợp của source code và cấu hình local.
    pub source_hash: String,
    /// Hashes of dependencies from other workspace packages.
    /// Danh sách hash dependency từ các package khác trong workspace.
    pub dependency_hashes: BTreeMap<String, String>,
    /// Combined computation hash đại diện cho toàn bộ build state
    pub composite_hash: String,
    /// Build completion timestamp in ISO 8601 format.
    /// Thời gian build hoàn tất theo định dạng ISO 8601.
    pub last_built_at: String,
}

/// Calculate the SHA-256 digest of a file.
/// Tính digest SHA-256 của một file.
pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let hasher = blake3_or_sha256(&bytes);
    Ok(hasher)
}

fn blake3_or_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Encode native path units so distinct source names cannot collapse to one cache key.
/// Mã hóa đơn vị path gốc để các tên nguồn khác nhau không bị gộp thành cùng cache key.
#[cfg(unix)]
fn source_path_key(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    encode_path_bytes(path.as_os_str().as_bytes().iter().copied())
}

#[cfg(windows)]
fn source_path_key(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    encode_path_bytes(path.as_os_str().encode_wide().flat_map(u16::to_le_bytes))
}

#[cfg(not(any(unix, windows)))]
fn source_path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn encode_path_bytes(bytes: impl IntoIterator<Item = u8>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::new();
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

/// Collect and hash package source files, excluding generated and cache directories.
/// Thu thập và hash source package, bỏ qua thư mục sinh tự động và cache.
pub fn compute_package_source_hash(package_root: &Path) -> Result<String> {
    // Refuse missing, linked, or non-directory roots so they cannot look like empty packages.
    // Từ chối root bị thiếu, là symlink hoặc không phải thư mục để không bị hiểu nhầm là package rỗng.
    let root_metadata = fs::symlink_metadata(package_root).with_context(|| {
        format!(
            "cannot inspect package source root '{}'",
            package_root.display()
        )
    })?;
    if root_metadata.file_type().is_symlink() {
        bail!(
            "package source root '{}' must not be a symlink",
            package_root.display()
        );
    }
    if !root_metadata.is_dir() {
        bail!(
            "package source root '{}' must be a directory",
            package_root.display()
        );
    }

    let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();
    let mut visited_directories = HashSet::new();
    walk_and_collect_hashes(
        package_root,
        package_root,
        &mut file_hashes,
        &mut visited_directories,
    )?;

    Ok(hash_source_entries(&file_hashes))
}

fn hash_source_entries(file_hashes: &BTreeMap<String, String>) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"mgc-package-source-hash-v2\0");
    hasher.update((file_hashes.len() as u64).to_be_bytes());
    for (rel_path, f_hash) in file_hashes {
        update_hash_part(&mut hasher, rel_path.as_bytes());
        update_hash_part(&mut hasher, f_hash.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn update_hash_part(hasher: &mut sha2::Sha256, bytes: &[u8]) {
    use sha2::Digest;

    // Prefix every field with its byte length to preserve unambiguous boundaries.
    // Thêm độ dài byte trước mỗi trường để ranh giới dữ liệu luôn rõ ràng.
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn walk_and_collect_hashes(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<String, String>,
    visited_directories: &mut HashSet<std::path::PathBuf>,
) -> Result<()> {
    // Keep traversal iterative and reject duplicate physical directories to avoid cycles/aliases.
    // Duyệt lặp và từ chối thư mục vật lý trùng nhau để tránh vòng lặp hoặc alias.
    let mut pending = vec![current.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let metadata = fs::symlink_metadata(&directory).with_context(|| {
            format!("cannot inspect source directory '{}'", directory.display())
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!(
                "source directory '{}' must be a real directory",
                directory.display()
            );
        }
        let canonical = fs::canonicalize(&directory).with_context(|| {
            format!("cannot resolve source directory '{}'", directory.display())
        })?;
        if !visited_directories.insert(canonical) {
            bail!(
                "source directory '{}' was reached more than once",
                directory.display()
            );
        }

        let entries = fs::read_dir(&directory)
            .with_context(|| format!("cannot read source directory '{}'", directory.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| {
                format!(
                    "cannot read an entry in source directory '{}'",
                    directory.display()
                )
            })?;
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let file_type = entry
                .file_type()
                .with_context(|| format!("cannot inspect source entry '{}'", path.display()))?;

            if file_type.is_symlink() {
                bail!("source entry '{}' must not be a symlink", path.display());
            }

            if file_type.is_dir() {
                // Generated folders are intentionally excluded from package source hashes.
                // Cố ý loại các thư mục sinh tự động khỏi hash mã nguồn package.
                if matches!(
                    name_str.as_ref(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | ".git"
                        | ".magicore"
                        | ".cache"
                        | ".turbo"
                        | ".next"
                        | "coverage"
                        | ".venv"
                ) {
                    continue;
                }
                pending.push(path);
            } else if file_type.is_file() {
                // Ignore known volatile files; all other read/hash errors must propagate.
                // Bỏ qua file biến động đã biết; mọi lỗi đọc/hash khác phải được trả về.
                if name_str.ends_with(".log")
                    || name_str.ends_with(".tmp")
                    || name_str.starts_with(".DS_Store")
                {
                    continue;
                }

                let rel = path.strip_prefix(root).with_context(|| {
                    format!("source entry '{}' escaped package root", path.display())
                })?;
                let rel_str = source_path_key(rel);
                let content_hash = hash_file(&path)
                    .with_context(|| format!("cannot hash source file '{}'", path.display()))?;
                if out.insert(rel_str, content_hash).is_some() {
                    bail!(
                        "source path key collision while hashing '{}'",
                        path.display()
                    );
                }
            } else {
                bail!(
                    "source entry '{}' is not a regular file or directory",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

/// Compute a composite hash from the source hash and dependency hashes.
/// Tính composite hash từ source_hash và hash của các dependency.
pub fn compute_composite_hash(source_hash: &str, dep_hashes: &BTreeMap<String, String>) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"mgc-package-composite-hash-v2\0");
    update_hash_part(&mut hasher, source_hash.as_bytes());
    hasher.update((dep_hashes.len() as u64).to_be_bytes());
    for (dep_name, dep_hash) in dep_hashes {
        update_hash_part(&mut hasher, dep_name.as_bytes());
        update_hash_part(&mut hasher, dep_hash.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

const CACHE_FILE_NAME: &str = ".mgc_build_cache.json";

/// Read the saved computation cache for a package.
/// Đọc cache computation đã lưu của package.
pub fn load_package_build_cache(package_root: &Path) -> Option<PackageBuildCache> {
    let cache_path = package_root.join(".magicore").join(CACHE_FILE_NAME);
    let content = fs::read_to_string(cache_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save a package computation cache after a successful build.
/// Lưu cache computation của package sau khi build thành công.
pub fn save_package_build_cache(
    package_root: &Path,
    source_hash: String,
    dependency_hashes: BTreeMap<String, String>,
) -> Result<PackageBuildCache> {
    let composite_hash = compute_composite_hash(&source_hash, &dependency_hashes);
    let cache_dir = package_root.join(".magicore");
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    let cache = PackageBuildCache {
        source_hash,
        dependency_hashes,
        composite_hash,
        last_built_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    };

    let content = serde_json::to_string_pretty(&cache)?;
    fs::write(cache_dir.join(CACHE_FILE_NAME), content)?;
    Ok(cache)
}

/// Check whether a package needs to be rebuilt.
/// Kiểm tra xem package có cần build lại hay không.
/// Return `(should_rebuild, current_source_hash, current_composite_hash)`.
/// Trả về `(should_rebuild, current_source_hash, current_composite_hash)`.
pub fn check_package_build_freshness(
    package_root: &Path,
    dependency_hashes: &BTreeMap<String, String>,
) -> Result<(bool, String, String)> {
    let current_source_hash = compute_package_source_hash(package_root)?;
    let current_composite_hash = compute_composite_hash(&current_source_hash, dependency_hashes);

    let Some(saved_cache) = load_package_build_cache(package_root) else {
        return Ok((true, current_source_hash, current_composite_hash));
    };

    if saved_cache.composite_hash == current_composite_hash {
        // The cache is fresh when neither source nor dependencies changed.
        // Cache còn mới nếu source và dependencies đều không thay đổi.
        Ok((false, current_source_hash, current_composite_hash))
    } else {
        Ok((true, current_source_hash, current_composite_hash))
    }
}

#[cfg(test)]
mod tests {
    use super::{compute_composite_hash, hash_source_entries, source_path_key};
    use std::collections::BTreeMap;

    #[test]
    fn source_hash_frames_path_and_content_fields() {
        let mut split_entries = BTreeMap::new();
        split_entries.insert("61".to_string(), "b".repeat(64));
        split_entries.insert("62".to_string(), "c".repeat(64));

        let mut combined_entry = BTreeMap::new();
        combined_entry.insert(format!("61{}62", "b".repeat(64)), "c".repeat(64));

        assert_ne!(
            hash_source_entries(&split_entries),
            hash_source_entries(&combined_entry),
            "different path/content boundaries must produce different source hashes"
        );
    }

    #[test]
    fn composite_hash_frames_dependency_fields() {
        let source_hash = "a".repeat(64);
        let mut split_dependencies = BTreeMap::new();
        split_dependencies.insert("a".to_string(), "b".repeat(64));
        split_dependencies.insert("c".to_string(), "d".repeat(64));

        let mut combined_dependency = BTreeMap::new();
        combined_dependency.insert(format!("a{}c", "b".repeat(64)), "d".repeat(64));

        assert_ne!(
            compute_composite_hash(&source_hash, &split_dependencies),
            compute_composite_hash(&source_hash, &combined_dependency),
            "different dependency boundaries must produce different composite hashes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn source_path_key_distinguishes_invalid_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;

        let path_a = Path::new(OsStr::from_bytes(b"source-\x80.ts"));
        let path_b = Path::new(OsStr::from_bytes(b"source-\x81.ts"));

        assert_ne!(source_path_key(path_a), source_path_key(path_b));
    }
}
