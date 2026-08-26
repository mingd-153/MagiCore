//! `install/mod.rs` — Lib adapter install orchestrator.
//! Orchestrates install across TypeScript (delegate web), Rust (cargo), Python (pip/uv).

pub mod fetch;
pub mod verify;

use mgc_store::ContentStore;
use mgc_types::adapter::{InstallOptions, InstallSummary, PackageAdapter};
use mgc_types::{MgError, MgResult, ResolvedGraph};
use std::path::Path;

use crate::language::LibLanguage;

/// Install orchestrator for lib adapter.
/// Điều phối install cho TypeScript/Rust/Python library projects.
pub(crate) async fn run_install(
    language: LibLanguage,
    web: Option<&mgc_web_adapter::WebAdapter>,
    graph: &ResolvedGraph,
    project_root: &Path,
    opts: InstallOptions,
    _store: Option<&ContentStore>,
) -> MgResult<InstallSummary> {
    match language {
        LibLanguage::Ts => {
            let web = web.ok_or_else(|| {
                MgError::Other("TypeScript lib requires web adapter delegate".to_string())
            })?;
            web.install(graph, project_root, opts).await
        }
        LibLanguage::Rust => install_rust(project_root, opts).await,
        LibLanguage::Python => install_python(project_root, opts).await,
    }
}

async fn install_rust(project_root: &Path, opts: InstallOptions) -> MgResult<InstallSummary> {
    let mut args = vec!["fetch".to_string()];
    if opts.frozen {
        args.push("--frozen".to_string());
    }

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("cargo", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("cargo fetch failed: {}", e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other(format!(
            "cargo fetch exited with code {}",
            result.exit_code
        )));
    }

    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: 0,
    })
}

async fn install_python(project_root: &Path, _opts: InstallOptions) -> MgResult<InstallSummary> {
    // Prefer uv over pip if available
    let tool = if which::which("uv").is_ok() {
        "uv"
    } else {
        "pip"
    };

    let args = vec!["install".to_string(), "-e".to_string(), ".".to_string()];

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run(tool, &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("{} install failed: {}", tool, e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other(format!(
            "{} install exited with code {}",
            tool, result.exit_code
        )));
    }

    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: 0,
    })
}
