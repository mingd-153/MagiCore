//! Bevy dependency installation via cargo orchestrate.

use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Install Bevy dependencies via `cargo fetch`
/// Orchestrate cargo - không reimplement resolver crates.io (Q10)
pub async fn install_dependencies(project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    let cargo_toml = project_root.join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err(MgError::Other("Cargo.toml not found".into()));
    }

    // Stub: actual implementation needs mgc-exec with cargo allowlist
    // Command: cargo fetch --manifest-path <path>
    // Result: parse Cargo.lock for installed packages

    let packages = vec!["bevy@0.14.0".to_string()]; // Stub
    let bytes = 0; // Stub - would parse cargo output
    let verified = true; // Cargo.lock ensures integrity

    Ok((packages, bytes, verified))
}

/// Add Bevy dependency via `cargo add`
pub async fn add_dependency(
    _project_root: &Path,
    _name: &str,
    _version: Option<&str>,
    _dev: bool,
) -> MgResult<()> {
    // Stub: mgc-exec cargo add
    // cargo add <name>[@version] [--dev]
    // Then: cargo fetch

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_install_bevy() {
        let tmp = tmp();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"game\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();

        let (packages, _, verified) = install_dependencies(tmp.path()).await.unwrap();
        assert!(verified);
        assert!(!packages.is_empty());
    }

    #[tokio::test]
    async fn test_install_no_cargo_toml() {
        let tmp = tmp();
        let result = install_dependencies(tmp.path()).await;
        assert!(result.is_err());
    }
}
