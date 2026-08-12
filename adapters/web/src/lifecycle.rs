use mg_types::error::{MgError, MgResult};
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_LIFECYCLE_TIMEOUT_SECS: u64 = 300;
const LIFECYCLE_TIMEOUT_ENV: &str = "MG_LIFECYCLE_TIMEOUT_SECS";

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
        let invocation = mg_exec::allowlist::parse_script_invocation(script)
            .map_err(|e| MgError::Other(format!("unsupported lifecycle script '{name}': {e}")))?;
        let path_env = lifecycle_path_env(project_root)?;
        let mut env = vec![
            ("PATH".to_string(), path_env.to_string_lossy().to_string()),
            ("INIT_CWD".to_string(), project_root.display().to_string()),
            ("npm_config_node_gyp".to_string(), "node-gyp".to_string()),
        ];
        env.extend(invocation.env);
        let opts = mg_exec::prelude::ExecOptions {
            cwd: Some(pkg_dir.to_path_buf()),
            timeout: Some(lifecycle_timeout()),
            env,
            clean_env: true,
            ..Default::default()
        };

        mg_exec::prelude::run(&invocation.program, &invocation.args, &opts)
            .map_err(|e| MgError::Other(format!("lifecycle script '{name}' failed: {e}")))?;

        Ok(())
    }
}

fn lifecycle_timeout() -> Duration {
    std::env::var(LIFECYCLE_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_LIFECYCLE_TIMEOUT_SECS))
}

fn reject_external_package_manager_script(script: &str, manifest_path: &Path) -> MgResult<()> {
    if let Some(pm) = mg_exec::allowlist::find_forbidden_tool_in_script(script) {
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

    fn write_package_script(package: &Path, script: &str) {
        let manifest = serde_json::json!({
            "scripts": {
                "postinstall": script,
            }
        });
        std::fs::write(package.join("package.json"), manifest.to_string()).unwrap();
    }

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

    #[test]
    fn lifecycle_rejects_pm_wrappers_after_shell_separators() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        std::fs::write(
            package.path().join("package.json"),
            r#"{"scripts":{"postinstall":"node build.js && /usr/bin/pnpm install"}}"#,
        )
        .unwrap();

        let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
        assert!(err.to_string().contains("delegates to 'pnpm'"));
    }

    #[test]
    fn lifecycle_rejects_shell_control_tokens() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        std::fs::write(
            package.path().join("package.json"),
            r#"{"scripts":{"postinstall":"node build.js; node post.js"}}"#,
        )
        .unwrap();

        let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
        assert!(err.to_string().contains("unsupported lifecycle script"));
    }

    #[test]
    #[cfg(unix)]
    fn lifecycle_runs_simple_script_without_shell() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        let marker = package.path().join("marker.txt");
        write_package_script(
            package.path(),
            "python3 -c \"from pathlib import Path; Path('marker.txt').write_text('ok')\"",
        );

        LifecycleRunner::run_scripts(package.path(), project.path()).unwrap();
        assert!(marker.exists());
    }

    #[test]
    #[cfg(unix)]
    fn lifecycle_accepts_leading_env_assignment() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        write_package_script(
            package.path(),
            "MG_LIFECYCLE_TEST=ok python3 -c \"import os; assert os.environ.get('MG_LIFECYCLE_TEST') == 'ok'\"",
        );

        LifecycleRunner::run_scripts(package.path(), project.path()).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn lifecycle_timeout_kills_hung_process() {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        write_package_script(package.path(), "python3 -c \"import time; time.sleep(2)\"");

        std::env::set_var("MG_LIFECYCLE_TIMEOUT_SECS", "1");
        let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
        std::env::remove_var("MG_LIFECYCLE_TIMEOUT_SECS");
        assert!(err.to_string().contains("timed out"));
    }
}
