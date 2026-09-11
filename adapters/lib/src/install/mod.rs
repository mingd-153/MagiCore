//! `install/mod.rs` — Lib adapter install orchestrator.
//! Orchestrates install across TypeScript (delegate web), Rust (cargo), Python (pip/uv).

pub mod fetch;
pub mod verify;

use mgc_store::ContentStore;
use mgc_types::adapter::{InstallCacheMode, InstallOptions, InstallSummary, PackageAdapter};
use mgc_types::{MgError, MgResult, ResolvedGraph};
use std::path::Path;
use std::time::Instant;

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
        // Go: `go mod download` fetches the pinned module set — the go
        // toolchain owns module caching (Q9-style delegation, no shim).
        // Go: `go mod download` tải tập module đã ghim — go toolchain giữ
        // module cache (ủy quyền kiểu Q9, không lệnh bọc).
        LibLanguage::Go => install_go(project_root, opts).await,
        // Java/.NET install is not wired yet (P2 audit parity scope):
        // gradle/dotnet own dependency fetching; mgc audits their
        // lockfiles. Honest failure, never a silent no-op summary.
        // Install Java/.NET chưa nối (scope parity audit P2): gradle/
        // dotnet giữ việc tải dependency; mgc audit lockfile của chúng.
        // Fail trung thực, không trả summary no-op âm thầm.
        LibLanguage::Java => Err(MgError::Other(
            "java install is delegated to gradle (mgc reads gradle/verification-metadata.xml for audits); the native java install lane lands with P2".to_string(),
        )),
        LibLanguage::DotNet => Err(MgError::Other(
            ".NET install is delegated to dotnet restore (mgc reads packages.lock.json for audits); the native .NET install lane lands with P2".to_string(),
        )),
    }
}

/// Fetch Go modules per go.mod — delegated to the go toolchain.
/// Tải module Go theo go.mod — ủy quyền cho go toolchain.
async fn install_go(project_root: &Path, _opts: InstallOptions) -> MgResult<InstallSummary> {
    // P0-6: delegated install — the go module cache owns the bytes, so
    // the summary says so instead of a silent zero byte-count.
    // P0-6: install ủy quyền — module cache của go giữ byte, summary
    // nói rõ điều đó thay vì byte-count 0 âm thầm.
    let started = Instant::now();
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };
    mgc_exec::run::run(
        "go",
        &["mod".to_string(), "download".to_string()],
        &exec_opts,
    )
    .map_err(|e| MgError::Other(format!("go mod download failed: {e}")))?;
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::Delegated,
    })
}

async fn install_rust(project_root: &Path, opts: InstallOptions) -> MgResult<InstallSummary> {
    let mut args = vec!["fetch".to_string()];
    if opts.frozen {
        args.push("--frozen".to_string());
    }

    let started = Instant::now();
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

    // P0-6: cargo's registry cache owns the bytes (delegation) — the
    // summary labels it Delegated; bytes_from_cache stays 0 BY DESIGN
    // (never a claim that nothing was cached).
    // P0-6: registry cache của cargo giữ byte (ủy quyền) — summary ghi
    // Delegated; bytes_from_cache = 0 THEO THIẾT KẾ (không phải claim
    // "không cache được gì").
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::Delegated,
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
    let started = Instant::now();

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

    // P0-6: uv/pip cache owns the bytes (delegation).
    // P0-6: cache của uv/pip giữ byte (ủy quyền).
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::Delegated,
    })
}
