use mg_types::error::{MgError, MgResult};
use serde::Deserialize;
use std::path::Path;
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

        let manifest: PackageManifest = serde_json::from_str(&contents).unwrap_or_else(|_| PackageManifest {
            scripts: PackageScripts::default(),
        });

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
        let node_modules_bin = project_root.join("node_modules").join(".bin");
        let mut path_env = std::env::var("PATH").unwrap_or_default();
        if node_modules_bin.exists() {
            path_env = format!("{}:{}", node_modules_bin.display(), path_env);
        }

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
            .env("npm_config_node_gyp", "node-gyp"); // Mock npm env vars some native addons require

        let status = cmd.status().map_err(|e| {
            MgError::Other(format!("failed to spawn {} script: {e}", name))
        })?;

        if !status.success() {
            return Err(MgError::Other(format!(
                "lifecycle script '{}' failed with status {}",
                name, status
            )));
        }

        Ok(())
    }
}
