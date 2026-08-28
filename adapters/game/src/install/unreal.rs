//! Unreal Engine dependency management (stub P1).

use mgc_types::MgResult;
use std::path::Path;

/// Install Unreal dependencies (stub - P1 scaffold-only)
/// Unreal dependency management complex - defer to P2
pub async fn install_dependencies(_project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    // Unreal P1 = scaffold-only (.uproject)
    // Dependency install (plugins, marketplace assets) = P2
    // For now, no-op like Godot

    Ok((vec![], 0, true))
}

/// Download Unreal Engine binary (stub)
pub async fn download_unreal_binary(
    version: &str,
    target_dir: &Path,
) -> MgResult<std::path::PathBuf> {
    // Stub: actual download requires Epic Games Launcher auth
    // Unreal Engine binary distribution complex (proprietary license)

    std::fs::create_dir_all(target_dir)?;
    let stub_path = target_dir.join(format!("UnrealEngine-{}.stub", version));
    std::fs::write(&stub_path, b"")?;

    Ok(stub_path)
}

#[cfg(test)]
#[path = "test/unreal_test.rs"]
mod tests;
