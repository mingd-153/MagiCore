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

/// Patch spec lưu trong mg.toml [patches] — chỉ npm-format (web/lib ts)
/// (A6: mg.lock patches CHỈ cho npm-format; core khác dùng lockfile chuẩn)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSpec {
    /// Package name (vd: "react", "@scope/pkg")
    pub package: String,
    /// Version range áp dụng patch (vd: "1.0.0 - 1.2.0")
    pub version_range: VersionRange,
    /// Đường dẫn patch file (tương đối trong ~/.megagate/patches/)
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

/// Patch record trong lockfile (mg.lock patches field — A6: npm-format only)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::VersionRange;

    #[test]
    fn patch_spec_serializes() {
        let vr = VersionRange::parse("^1.0.0").unwrap();
        let spec = PatchSpec::new(
            "react".into(),
            vr,
            "patches/react.patch".into(),
            "sha256-abc".into(),
        );
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("react"));
        assert!(json.contains("sha256-abc"));
        let back: PatchSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, PatchKind::Diff);
    }

    #[test]
    fn lock_patch_serializes() {
        let lp = LockPatch::new(
            "react".into(),
            "1.0.0".into(),
            "sha256-def".into(),
            "2026-01-01T00:00:00Z".into(),
        );
        let json = serde_json::to_string(&lp).unwrap();
        let back: LockPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "react");
        assert_eq!(back.sha256, "sha256-def");
    }
}
