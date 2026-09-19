//! Godot engine binary management.

use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Install Godot dependencies — fail closed: Godot has no standard
/// package manager (assets live in project.godot / are fetched
/// manually), so a silent Ok no-op would fake an install that never
/// happened. The adapter already rejects this path with the same
/// guidance; this keeps the helper honest when called directly.
/// (Godot không có PM chuẩn — fail-closed thay vì no-op im lặng.)
pub async fn install_dependencies(_project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    Err(MgError::Unsupported {
        core: "game",
        capability: "install",
        guidance:
            "Godot projects have no dependency install step; open the project in the Godot editor"
                .to_string(),
    })
}

/// Download Godot binary for specific version — NOT automated (fail
/// closed): writing an empty file and calling it a download fakes an
/// artifact. Fetch it from https://github.com/godotengine/godot/releases
/// (Godot_v{version}_linux.x86_64 / _macos.universal / _win64.exe).
/// (Chưa tự động tải binary Godot — fail-closed thay vì ghi file rỗng.)
pub async fn download_godot_binary(
    _version: &str,
    _target_dir: &Path,
) -> MgResult<std::path::PathBuf> {
    Err(MgError::Other(
        "downloading the Godot editor binary is not automated — fetch it from https://github.com/godotengine/godot/releases".to_string(),
    ))
}

#[cfg(test)]
#[path = "test/godot_test.rs"]
mod tests;
