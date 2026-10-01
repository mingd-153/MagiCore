// Web lifecycle execution guard — validates and runs package lifecycle scripts.
// Bộ chạy lifecycle cho core-web — giữ script package trong allowlist an toàn.
use mgc_types::error::{MgError, MgResult};
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_LIFECYCLE_TIMEOUT_SECS: u64 = 300;
const LIFECYCLE_TIMEOUT_ENV: &str = "MGC_LIFECYCLE_TIMEOUT_SECS";

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct PackageScripts {
    pub(crate) preinstall: Option<String>,
    pub(crate) install: Option<String>,
    pub(crate) postinstall: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackageManifest {
    #[serde(default)]
    scripts: PackageScripts,
}

pub struct LifecycleRunner;

impl PackageScripts {
    pub(crate) fn has_hooks(&self) -> bool {
        self.preinstall.is_some() || self.install.is_some() || self.postinstall.is_some()
    }
}

/// Read lifecycle scripts from a no-follow, size-bounded manifest snapshot.
/// Policy inspection and execution must share this exact parsed value.
/// Đọc snapshot không theo symlink; policy và thực thi phải dùng cùng dữ liệu.
pub(crate) fn load_package_scripts(pkg_dir: &Path) -> MgResult<PackageScripts> {
    let package_json = pkg_dir.join("package.json");
    let contents =
        mgc_config::project::read_regular_project_text(&package_json, "installed package manifest")
            .map_err(|error| {
                MgError::Other(format!(
                    "failed to safely read package.json for lifecycle '{}': {error}",
                    package_json.display()
                ))
            })?;
    let Some(contents) = contents else {
        return Ok(PackageScripts::default());
    };
    let manifest: PackageManifest = serde_json::from_str(&contents).map_err(|error| {
        MgError::Other(format!(
            "failed to parse package.json for lifecycle '{}': {error}",
            package_json.display()
        ))
    })?;
    Ok(manifest.scripts)
}

impl LifecycleRunner {
    pub fn run_scripts(pkg_dir: &Path, project_root: &Path) -> MgResult<()> {
        let scripts = load_package_scripts(pkg_dir)?;
        Self::run_scripts_with_snapshot(pkg_dir, project_root, scripts)
    }

    pub(crate) fn run_scripts_with_snapshot(
        pkg_dir: &Path,
        project_root: &Path,
        scripts: PackageScripts,
    ) -> MgResult<()> {
        let owner_before =
            mgc_config::project::ProjectConfig::detect_core(project_root).map_err(|error| {
                MgError::Other(format!(
                    "cannot validate project core identity before lifecycle scripts: {error}"
                ))
            })?;
        let identity_snapshot = mgc_config::project::snapshot_project_identity(project_root)
            .map_err(|error| {
                MgError::Other(format!(
                    "cannot snapshot project identity before lifecycle scripts: {error}"
                ))
            })?;
        // Test-only deterministic failure injector (transaction E2E):
        // MGC_LIFECYCLE_FAIL_PACKAGES names package dirs (comma-separated,
        // "*" matches all) whose scripts must FAIL before running anything.
        // No side effects — the test proves transaction boundaries, not
        // script behavior. Env-gated like failpoints: whoever controls the
        // process environment already controls the process.
        // (Móc lỗi deterministic chỉ-cho-test cho E2E transaction.)
        if let Ok(filter) = std::env::var("MGC_LIFECYCLE_FAIL_PACKAGES") {
            let dir_name = pkg_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if filter
                .split(',')
                .map(str::trim)
                .any(|f| f == "*" || f == dir_name)
            {
                return Err(MgError::Other(format!(
                    "lifecycle injected failure for '{dir_name}' (MGC_LIFECYCLE_FAIL_PACKAGES)"
                )));
            }
        }
        if let Some(script) = scripts.preinstall {
            Self::run_script(pkg_dir, project_root, "preinstall", &script)?;
            Self::ensure_project_identity(project_root, &owner_before, &identity_snapshot)?;
        }
        if let Some(script) = scripts.install {
            Self::run_script(pkg_dir, project_root, "install", &script)?;
            Self::ensure_project_identity(project_root, &owner_before, &identity_snapshot)?;
        }
        if let Some(script) = scripts.postinstall {
            Self::run_script(pkg_dir, project_root, "postinstall", &script)?;
            Self::ensure_project_identity(project_root, &owner_before, &identity_snapshot)?;
        }

        Self::ensure_project_identity(project_root, &owner_before, &identity_snapshot)?;

        Ok(())
    }

    fn ensure_project_identity(
        project_root: &Path,
        expected_owner: &Option<String>,
        identity_snapshot: &mgc_config::project::ProjectIdentitySnapshot,
    ) -> MgResult<()> {
        let identity_matches =
            mgc_config::project::project_identity_matches_snapshot(project_root, identity_snapshot)
                .map_err(|error| {
                    let restore = mgc_config::project::restore_project_identity(
                        project_root,
                        identity_snapshot,
                    );
                    match restore {
                        Ok(()) => MgError::Other(format!(
                            "project core identity could not be verified during lifecycle scripts and was restored: {error}"
                        )),
                        Err(restore_error) => MgError::Other(format!(
                            "project core identity could not be verified during lifecycle scripts: {error}; restoration also failed: {restore_error}"
                        )),
                    }
                })?;
        if !identity_matches {
            let restore =
                mgc_config::project::restore_project_identity(project_root, identity_snapshot);
            let detail = match restore {
                Ok(()) => "; original identity files were restored".to_owned(),
                Err(error) => format!("; restoration also failed: {error}"),
            };
            return Err(MgError::Other(format!(
                "lifecycle scripts modified project core identity files; install aborted{detail}"
            )));
        }
        let owner_after =
            mgc_config::project::ProjectConfig::detect_core(project_root).map_err(|error| {
                let restore = mgc_config::project::restore_project_identity(
                    project_root,
                    identity_snapshot,
                );
                match restore {
                    Ok(()) => MgError::Other(format!(
                        "project core identity became invalid during lifecycle scripts and was restored: {error}"
                    )),
                    Err(restore_error) => MgError::Other(format!(
                        "project core identity became invalid during lifecycle scripts: {error}; restoration also failed: {restore_error}"
                    )),
                }
            })?;
        if owner_after != *expected_owner {
            let restore =
                mgc_config::project::restore_project_identity(project_root, identity_snapshot);
            let detail = match restore {
                Ok(()) => "; original identity files were restored".to_owned(),
                Err(error) => format!("; restoration also failed: {error}"),
            };
            return Err(MgError::Other(format!(
                "lifecycle scripts changed project core identity from '{}' to '{}'; install aborted{detail}",
                expected_owner.as_deref().unwrap_or("unclaimed"),
                owner_after.as_deref().unwrap_or("unclaimed"),
            )));
        }
        Ok(())
    }

    fn run_script(pkg_dir: &Path, project_root: &Path, name: &str, script: &str) -> MgResult<()> {
        reject_external_package_manager_script(script, &pkg_dir.join("package.json"))?;
        let invocation = mgc_exec::allowlist::parse_script_invocation(script)
            .map_err(|e| MgError::Other(format!("unsupported lifecycle script '{name}': {e}")))?;
        let path_env = lifecycle_path_env(project_root)?;
        let mut env = vec![
            ("PATH".to_string(), path_env.to_string_lossy().to_string()),
            ("INIT_CWD".to_string(), project_root.display().to_string()),
            ("npm_config_node_gyp".to_string(), "node-gyp".to_string()),
        ];
        env.extend(invocation.env);
        let opts = mgc_exec::prelude::ExecOptions {
            cwd: Some(pkg_dir.to_path_buf()),
            timeout: Some(lifecycle_timeout()),
            env,
            clean_env: true,
            ..Default::default()
        };

        mgc_exec::prelude::run(&invocation.program, &invocation.args, &opts)
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
    if let Some(pm) = mgc_exec::allowlist::find_forbidden_tool_in_script(script) {
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
#[path = "test/lifecycle_tests.rs"]
mod tests;
