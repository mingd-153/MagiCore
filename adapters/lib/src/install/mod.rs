//! `install/mod.rs` — Lib adapter install orchestrator.
//! Orchestrates install across TypeScript (delegate web), Rust (cargo), Python (pip/uv).

pub mod fetch;
// pub for integration tests (cross-project reuse proof); semver —
// internal surface, do not use outside this crate's tests.
// pub cho integration test (chứng minh tái sử dụng chéo project);
// bề mặt nội bộ — không dùng ngoài test của crate này.
pub mod shared_store;
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

    // Shared store (B-series, 2026-09-12): CARGO_HOME points INSIDE the
    // mgc store (~/.magicore/store/cargo) so every project reuses the
    // same registry bytes — cross-project, cross-workspace. The layout
    // stays cargo-native; mgc owns the directory and measures reuse.
    // Store chia sẻ (B-series): CARGO_HOME trỏ VÀO store của mgc
    // (~/.magicore/store/cargo) nên mọi project dùng lại cùng byte
    // registry — chéo project, chéo workspace. Layout giữ nguyên của
    // cargo; mgc giữ thư mục và đo mức tái sử dụng.
    let store = shared_store::SharedStoreRun::cargo()?;
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        env: store.env_vars(),
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

    let (reused, duration_ms, cache_mode) = store.finish();
    // The shared mgc store owned every byte: reuse is measured across
    // projects; when the store grew, the fresh delta was downloaded;
    // when it did not grow, the whole byte set was served from the
    // store. Either way the label is MgCStore (the directory is ours).
    // Store chung của mgc giữ mọi byte: tái sử dụng được đo chéo
    // project; store lớn thêm nghĩa là có delta mới tải; không lớn
    // nghĩa là toàn bộ byte được phục vụ từ store. Nhãn luôn MgCStore
    // (thư mục là của mgc).
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: reused,
        duration_ms,
        cache_mode,
    })
}
async fn install_python(project_root: &Path, _opts: InstallOptions) -> MgResult<InstallSummary> {
    // Python install contract (aligned with the ai core lane, P0 fix
    // 2026-09-12): uv has NO `uv install` subcommand. With a uv.lock
    // the honest sync is `uv sync` (toolchain-native, same as the ai
    // core); WITHOUT a lock the fallback is `uv pip install --system
    // -e .` (uv needs a target env — --system is explicit, never a
    // silent venv guess). pip-only environments keep the classic
    // `pip install -e .`.
    // Hợp đồng install Python (canh theo lane core ai, P0 fix): uv
    // KHÔNG có subcommand `uv install`. Có uv.lock thì sync trung thực
    // là `uv sync` (toolchain-native, như core ai); KHÔNG có lock thì
    // fallback `uv pip install --system -e .` (uv cần env đích —
    // --system tường minh, không đoán venv âm thầm). Môi trường chỉ
    // pip giữ `pip install -e .` cổ điển.
    let has_uv = which::which("uv").is_ok();
    let (tool, args): (&str, Vec<String>) = if has_uv && project_root.join("uv.lock").is_file() {
        ("uv", vec!["sync".to_string()])
    } else if has_uv {
        (
            "uv",
            vec![
                "pip".to_string(),
                "install".to_string(),
                "--system".to_string(),
                "-e".to_string(),
                ".".to_string(),
            ],
        )
    } else {
        (
            "pip",
            vec!["install".to_string(), "-e".to_string(), ".".to_string()],
        )
    };

    // Shared store (B-series): PIP_CACHE_DIR/UV_CACHE_DIR point inside
    // the mgc store (~/.magicore/store/pypi) — every python project on
    // this machine shares the same wheel/sdist cache bytes.
    // Store chia sẻ (B-series): PIP_CACHE_DIR/UV_CACHE_DIR trỏ vào
    // store mgc (~/.magicore/store/pypi) — mọi project python trên máy
    // chia sẻ cùng byte cache wheel/sdist.
    let store = shared_store::SharedStoreRun::pypi()?;
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        env: store.env_vars(),
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

    let (reused, duration_ms, cache_mode) = store.finish();
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: reused,
        duration_ms,
        cache_mode,
    })
}
