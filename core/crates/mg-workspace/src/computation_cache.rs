use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Metadata lưu trữ cache computation của từng package trong workspace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageBuildCache {
    /// Hash tổng hợp của source code + local config
    pub source_hash: String,
    /// Danh sách hash phụ thuộc từ các workspace packages khác
    pub dependency_hashes: BTreeMap<String, String>,
    /// Combined computation hash đại diện cho toàn bộ build state
    pub composite_hash: String,
    /// Thời gian build ISO string
    pub last_built_at: String,
}

/// Tính toán SHA-256 hash của một file
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

/// Thu thập toàn bộ file mã nguồn trong package để hash (bỏ qua target, node_modules, .git, v.v.)
pub fn compute_package_source_hash(package_root: &Path) -> Result<String> {
    let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();
    walk_and_collect_hashes(package_root, package_root, &mut file_hashes)?;

    // Băm toàn bộ map (rel_path -> hash) thành 1 hash duy nhất đại diện cho package
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for (rel_path, f_hash) in &file_hashes {
        hasher.update(rel_path.as_bytes());
        hasher.update(f_hash.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn walk_and_collect_hashes(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<String, String>,
) -> Result<()> {
    if !current.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(current)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Bỏ qua các thư mục artifact / cache
        if path.is_dir() {
            if matches!(
                name_str.as_ref(),
                "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | ".git"
                    | ".megagate"
                    | ".cache"
                    | ".turbo"
                    | ".next"
                    | "coverage"
                    | ".venv"
            ) {
                continue;
            }
            walk_and_collect_hashes(root, &path, out)?;
        } else if path.is_file() {
            // Bỏ qua các file tạm / lock file thay đổi liên tục
            if name_str.ends_with(".log")
                || name_str.ends_with(".tmp")
                || name_str.starts_with(".DS_Store")
            {
                continue;
            }

            if let Ok(rel) = path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().to_string();
                if let Ok(content_hash) = hash_file(&path) {
                    out.insert(rel_str, content_hash);
                }
            }
        }
    }
    Ok(())
}

/// Tính composite hash từ source_hash và dependencies' hashes
pub fn compute_composite_hash(
    source_hash: &str,
    dep_hashes: &BTreeMap<String, String>,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(source_hash.as_bytes());
    for (dep_name, dep_hash) in dep_hashes {
        hasher.update(dep_name.as_bytes());
        hasher.update(dep_hash.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

const CACHE_FILE_NAME: &str = ".mg_build_cache.json";

/// Đọc cache đã lưu của package
pub fn load_package_build_cache(package_root: &Path) -> Option<PackageBuildCache> {
    let cache_path = package_root.join(".megagate").join(CACHE_FILE_NAME);
    let content = fs::read_to_string(cache_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Lưu cache computation của package sau khi build thành công
pub fn save_package_build_cache(
    package_root: &Path,
    source_hash: String,
    dependency_hashes: BTreeMap<String, String>,
) -> Result<PackageBuildCache> {
    let composite_hash = compute_composite_hash(&source_hash, &dependency_hashes);
    let cache_dir = package_root.join(".megagate");
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

/// Kiểm tra xem package có cần build lại hay không
/// Trả về (should_rebuild, current_source_hash, current_composite_hash)
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
        // Cache hoàn toàn fresh (không đổi source, không đổi dependencies)
        Ok((false, current_source_hash, current_composite_hash))
    } else {
        Ok((true, current_source_hash, current_composite_hash))
    }
}
