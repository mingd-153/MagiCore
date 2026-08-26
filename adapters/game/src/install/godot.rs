//! Godot engine binary management.

use mgc_types::MgResult;
use std::path::Path;

/// Install Godot dependencies (no-op - editor manages assets)
/// Godot không có PM chuẩn - assets managed by editor
pub async fn install_dependencies(_project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    // Godot project không có dependency install như npm/cargo
    // Assets/addons được quản lý trong project.godot hoặc tải manual
    // mgc install cho godot = no-op

    Ok((vec![], 0, true))
}

/// Download Godot binary for specific version
pub async fn download_godot_binary(
    version: &str,
    target_dir: &Path,
) -> MgResult<std::path::PathBuf> {
    // Stub: actual download từ https://github.com/godotengine/godot/releases
    // Format: Godot_v{version}_linux.x86_64 / Godot_v{version}_macos.universal / Godot_v{version}_win64.exe

    std::fs::create_dir_all(target_dir)?;

    let binary_name = if cfg!(target_os = "macos") {
        format!("Godot_v{}_macos.universal", version)
    } else if cfg!(target_os = "windows") {
        format!("Godot_v{}_win64.exe", version)
    } else {
        format!("Godot_v{}_linux.x86_64", version)
    };

    let binary_path = target_dir.join(&binary_name);

    // Stub: create empty file for testing
    std::fs::write(&binary_path, b"")?;

    Ok(binary_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_install_godot() {
        let tmp = tmp();
        let (packages, bytes, verified) = install_dependencies(tmp.path()).await.unwrap();

        assert_eq!(packages.len(), 0);
        assert_eq!(bytes, 0);
        assert!(verified);
    }

    #[tokio::test]
    async fn test_download_godot_binary() {
        let tmp = tmp();
        let binary = download_godot_binary("4.3.0", tmp.path()).await.unwrap();

        assert!(binary.exists());
        assert!(binary.to_string_lossy().contains("Godot_v4.3.0"));
    }
}
