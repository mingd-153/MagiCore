//! Unreal Engine dependency management (fail-closed P1).

use mgc_types::MgResult;
use std::path::Path;

/// Install Unreal dependencies — NOT implemented (P1 scaffold-only).
/// Plugin/marketplace install requires the Epic Launcher; fail closed.
/// Cài dependency Unreal — CHƯA implement (P1 chỉ scaffold). Plugin/
/// marketplace cần Epic Launcher — fail-closed, không trả verified=true giả.
pub async fn install_dependencies(_project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    Err(mgc_types::MgError::Unsupported {
        core: "game",
        capability: "install dependencies (unreal)",
        guidance: "Unreal plugin/marketplace install requires the Epic \
                   Launcher; open the project in Unreal Editor"
            .to_string(),
    })
}

/// Download Unreal Engine binary — NOT implemented: requires Epic Games
/// Launcher auth (proprietary). Fails closed instead of writing a fake
/// `.stub` file that pretends to be the engine.
/// Tải binary Unreal — CHƯA implement: cần auth Epic Launcher (license
/// độc quyền). Fail-closed thay vì ghi file `.stub` giả là engine.
pub async fn download_unreal_binary(
    _version: &str,
    _target_dir: &Path,
) -> MgResult<std::path::PathBuf> {
    Err(mgc_types::MgError::Unsupported {
        core: "game",
        capability: "download engine binary (unreal)",
        guidance: "Unreal Engine distribution requires the Epic Games Launcher \
                   and a proprietary license; install it manually from Epic"
            .to_string(),
    })
}

#[cfg(test)]
#[path = "test/unreal_test.rs"]
mod tests;
