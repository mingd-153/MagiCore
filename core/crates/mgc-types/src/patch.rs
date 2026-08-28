//! Patch types — Package patch spec + lockfile patch record (16 §3-4)
//! (Patch: vá lỗi package như pnpm patchedDependencies — nội bộ, không gọi binary patch)

use crate::package::VersionRange;
use serde::{Deserialize, Serialize};

/// Patch kind — P1: Diff (apply engine nội bộ); P2: Prebuilt binary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PatchKind {
    #[default]
    Diff,
    Prebuilt,
}

/// Patch spec lưu trong mgc.toml [patches] — chỉ npm-format (web/lib ts)
/// (A6: mgc.lock patches CHỈ cho npm-format; core khác dùng lockfile chuẩn)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSpec {
    /// Package name (vd: "react", "@scope/pkg")
    pub package: String,
    /// Version range áp dụng patch (vd: "1.0.0 - 1.2.0")
    pub version_range: VersionRange,
    /// Đường dẫn patch file (tương đối trong ~/.magicore/patches/)
    pub patch_path: String,
    /// SHA256 hash của nội dung diff — verify integrity
    pub integrity: String,
    /// Kind: Diff (mặc định) hoặc Prebuilt
    #[serde(default)]
    pub kind: PatchKind,
}

impl PatchSpec {
    pub fn new(
        package: String,
        version_range: VersionRange,
        patch_path: String,
        integrity: String,
    ) -> Self {
        Self {
            package,
            version_range,
            patch_path,
            integrity,
            kind: PatchKind::Diff,
        }
    }
}

/// Patch record trong lockfile (mgc.lock patches field — A6: npm-format only)
/// Ghi lại package đã patch + hash sau patch + thời gian
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockPatch {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub applied_at: String, // ISO8601
}

impl LockPatch {
    pub fn new(name: String, version: String, sha256: String, applied_at: String) -> Self {
        Self {
            name,
            version,
            sha256,
            applied_at,
        }
    }
}
