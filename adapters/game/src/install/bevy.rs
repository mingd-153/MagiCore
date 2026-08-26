//! Bevy dependency installation via cargo orchestrate.

use mgc_exec::run::{run as mgc_run, ExecOptions};
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Install Bevy dependencies via `cargo fetch`
/// Orchestrate cargo - không reimplement resolver crates.io (Q10)
pub async fn install_dependencies(project_root: &Path) -> MgResult<(Vec<String>, u64, bool)> {
    let cargo_toml = project_root.join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err(MgError::Other("Cargo.toml not found".into()));
    }

    // Orchestrate thật qua mgc-exec (allowlist cargo, audit log, cwd = project)
    let opts = ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };
    let report = mgc_run("cargo", &["fetch".to_string()], &opts)?;
    if report.exit_code != 0 {
        return Err(MgError::Other(format!(
            "cargo fetch exited with {}: {}",
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }

    // Đếm packages từ Cargo.lock (nếu có) — fail-open về 0
    let lock = std::fs::read_to_string(project_root.join("Cargo.lock")).unwrap_or_default();
    let packages: Vec<String> = lock
        .lines()
        .filter_map(|l| l.strip_prefix("name = "))
        .map(|n| n.trim_matches('"').to_string())
        .collect();

    let verified = project_root.join("Cargo.lock").exists();
    Ok((packages, report.duration_ms as u64, verified))
}

/// Add Bevy dependency via `cargo add`
pub async fn add_dependency(
    project_root: &Path,
    name: &str,
    version: Option<&str>,
    dev: bool,
) -> MgResult<()> {
    let spec = match version {
        Some(v) => format!("{name}@{v}"),
        None => name.to_string(),
    };
    let mut args: Vec<String> = vec!["add".into(), spec];
    if dev {
        args.push("--dev".into());
    }
    let opts = ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };
    let report = mgc_run("cargo", &args, &opts)?;
    if report.exit_code != 0 {
        return Err(MgError::Other(format!(
            "cargo add exited with {}: {}",
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }
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
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
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
