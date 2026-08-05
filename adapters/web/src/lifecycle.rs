use mg_types::error::{MgError, MgResult};
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize, Default)]
struct PackageScripts {
    preinstall: Option<String>,
    install: Option<String>,
    postinstall: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackageManifest {
    #[serde(default)]
    scripts: PackageScripts,
}

pub struct LifecycleRunner;

impl LifecycleRunner {
    pub fn run_scripts(pkg_dir: &Path, project_root: &Path) -> MgResult<()> {
        let package_json = pkg_dir.join("package.json");
        if !package_json.exists() {
            return Ok(());
        }

        let contents = std::fs::read_to_string(&package_json).map_err(|e| {
            MgError::Other(format!("failed to read package.json for lifecycle: {e}"))
        })?;

        let manifest: PackageManifest = serde_json::from_str(&contents).map_err(|e| {
            MgError::Other(format!(
                "failed to parse package.json for lifecycle '{}': {e}",
                package_json.display()
            ))
        })?;

        if let Some(script) = manifest.scripts.preinstall {
            Self::run_script(pkg_dir, project_root, "preinstall", &script)?;
        }
        if let Some(script) = manifest.scripts.install {
            Self::run_script(pkg_dir, project_root, "install", &script)?;
        }
        if let Some(script) = manifest.scripts.postinstall {
            Self::run_script(pkg_dir, project_root, "postinstall", &script)?;
        }

        Ok(())
    }

    fn run_script(pkg_dir: &Path, project_root: &Path, name: &str, script: &str) -> MgResult<()> {
        reject_external_package_manager_script(script, &pkg_dir.join("package.json"))?;
        let path_env = lifecycle_path_env(project_root)?;

        // On Windows we should probably use cmd.exe, on Unix sh
        #[cfg(unix)]
        let mut cmd = Command::new("sh");
        #[cfg(unix)]
        cmd.arg("-c").arg(script);

        #[cfg(not(unix))]
        let mut cmd = Command::new("cmd");
        #[cfg(not(unix))]
        cmd.arg("/c").arg(script);

        cmd.current_dir(pkg_dir)
            .env("PATH", path_env)
            .env("INIT_CWD", project_root.display().to_string())
            .env("npm_config_node_gyp", "node-gyp");

        let status = cmd
            .status()
            .map_err(|e| MgError::Other(format!("failed to spawn {} script: {e}", name)))?;

        if !status.success() {
            return Err(MgError::Other(format!(
                "lifecycle script '{}' failed with status {}",
                name, status
            )));
        }

        Ok(())
    }
}

fn reject_external_package_manager_script(script: &str, manifest_path: &Path) -> MgResult<()> {
    let first = script.split_whitespace().next().unwrap_or_default();
    let delegated_pm = match first {
        "npm" => Some("npm"),
        "pnpm" => Some("pnpm"),
        "bun" => Some("bun"),
        "yarn" => Some("yarn"),
        "npx" => Some("npx"),
        "bunx" => Some("bunx"),
        _ => None,
    };

    if let Some(pm) = delegated_pm {
        return Err(MgError::Other(format!(
            "lifecycle script in '{}' delegates to '{}'; core-web refuses package-manager wrappers inside lifecycle execution",
            manifest_path.display(),
            pm
        )));
    }

    Ok(())
}

fn lifecycle_path_env(project_root: &Path) -> MgResult<OsString> {
    let node_modules_bin = project_root.join("node_modules").join(".bin");
    let current_paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();

    if !node_modules_bin.exists() {
        return std::env::join_paths(current_paths)
            .map_err(|e| MgError::Other(format!("failed to build lifecycle PATH: {e}")));
    }

    let mut paths = Vec::with_capacity(current_paths.len() + 1);
    paths.push(node_modules_bin);
    paths.extend(current_paths);
    std::env::join_paths(paths)
        .map_err(|e| MgError::Other(format!("failed to build lifecycle PATH: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_path_env_prepends_node_modules_bin_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("node_modules").join(".bin");
        std::fs::create_dir_all(&bin).unwrap();

        let path_env = lifecycle_path_env(dir.path()).unwrap();
        let paths: Vec<_> = std::env::split_paths(&path_env).collect();

        assert_eq!(paths.first(), Some(&bin));
    }

    #[test]
    fn lifecycle_errors_on_invalid_package_json() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        std::fs::write(package.path().join("package.json"), "{not-json").unwrap();

        let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();

        assert!(
            err.to_string()
                .contains("failed to parse package.json for lifecycle"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn lifecycle_rejects_external_package_manager_wrappers() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        std::fs::write(
            package.path().join("package.json"),
            r#"{"scripts":{"postinstall":"npm run postinstall:inner"}}"#,
        )
        .unwrap();

        let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
        assert!(err.to_string().contains("delegates to 'npm'"));
    }
}
