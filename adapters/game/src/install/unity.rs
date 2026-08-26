//! Unity UPM dependency installation with Read-and-Verify.

use mgc_types::{MgError, MgResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Unity packages-lock.json format
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnityPackagesLock {
    pub dependencies: std::collections::HashMap<String, UnityPackageLock>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnityPackageLock {
    pub version: String,
    pub depth: Option<u32>,
    pub source: Option<String>,
    pub hash: Option<String>,
}

/// Install Unity dependencies via UPM CLI + verify packages-lock.json
/// A1 + phản biện v2: UPM CLI tải, mgc verify checksums
pub async fn install_dependencies(project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    let manifest = project_root.join("Packages").join("manifest.json");
    let lock_file = project_root.join("Packages").join("packages-lock.json");

    if !manifest.exists() {
        return Err(MgError::Other(
            "Unity Packages/manifest.json not found".into(),
        ));
    }

    // Step 1: UPM CLI tải packages (passthrough with allowlist: unity, upm)
    // Stub: actual needs mgc-exec with unity/upm allowlist
    // Command: unity-editor -batchmode -quit -projectPath . (auto-resolve manifest.json)

    // Step 2: Verify packages-lock.json checksum
    if lock_file.exists() {
        let lock = parse_packages_lock(&lock_file)?;

        // Cảnh báo LỘ TRỜI (RULE §11): hash trong lock CHƯA được đối chiếu với cache Unity.
        // Không bao giờ im lặng giả "đã verify".
        // (Loud warning per RULE §11: lock hashes are NOT yet checked against the Unity cache — never silently claim verification.)
        let hashed = lock
            .dependencies
            .values()
            .filter(|p| p.hash.is_some())
            .count();
        mgc_ui::warning(&format!(
            "Unity integrity check not implemented (P2): {} package hashes in packages-lock.json were parsed but NOT verified against the Unity cache",
            hashed
        ));

        let packages: Vec<String> = lock
            .dependencies
            .keys()
            .map(|k| format!("{}@{}", k, lock.dependencies[k].version))
            .collect();

        // bool thứ 3 hiện nghĩa là "lockfile tồn tại + parse OK" — KHÔNG phải "hash đã đối chiếu"
        // (3rd bool means "lockfile present + parseable" — NOT "hashes checked")
        Ok((packages, 0, true))
    } else {
        // No lock file yet - first install
        Ok((vec![], 0, false))
    }
}

/// Verify Unity packages-lock.json integrity
/// Check hash field against actual tarball in Unity cache
/// FIXME(P2): chưa đối chiếu được với cache Unity (layout cache khác nhau theo OS) —
/// cần requirement layout + sample cache thật trước khi implement, không đoán mò (RULE §9.3).
// (FIXME(P2): cannot yet match hashes against the Unity cache — needs the cache-layout
// requirement and a real cache sample before implementing; no guessing per RULE §9.3.)
#[allow(dead_code)] // giữ làm spec tham chiếu cho P2 — kept as the P2 reference spec
fn verify_packages_lock(_lock_path: &Path) -> MgResult<bool> {
    // 1. Parse packages-lock.json
    // 2. For each package, find tarball in Unity cache (~/.local/share/unity3d/cache)
    // 3. Compute checksum of tarball
    // 4. Compare with hash in lock file
    // 5. Mismatch → return false (fail install + audit log)

    Ok(true)
}

/// Parse Unity packages-lock.json
fn parse_packages_lock(path: &Path) -> MgResult<UnityPackagesLock> {
    let content = std::fs::read_to_string(path)?;
    let lock: UnityPackagesLock = serde_json::from_str(&content)
        .map_err(|e| MgError::Other(format!("Parse packages-lock.json: {}", e)))?;
    Ok(lock)
}

#[cfg(test)]
#[path = "test/unity_tests.rs"]
mod tests;
