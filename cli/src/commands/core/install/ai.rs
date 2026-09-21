//! `mgc install` ai — tách từ core/ai.rs (Phase 7 v5). Lock ghép: uv.lock → pip requirements.lock.

use anyhow::Result;

use super::super::shared;

pub async fn install(
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::ai_project_root()?;
    if !packages.is_empty() {
        mgc_ui::info(&format!(
            "[ai install] ignoring package args {:?}; install is driven by project lock files (05 §5)",
            packages
        ));
    }
    // Native lane (mgc.lock, no uv.lock): a pyproject.toml means python
    // deps resolve through the NATIVE PyPI pipeline (same engine as
    // lib/python) — parse → resolve → verified fetch → mgc.lock. Zero
    // uv/pip spawn. uv.lock/requirements-only projects keep the legacy
    // delegated lane below (explicit compat).
    // (Lane native: pyproject → pipeline PyPI native, không spawn uv/pip.)
    if root.join("pyproject.toml").is_file() {
        return install_python_native(&root, dry_run, compat_runtime).await;
    }
    let (tool, args) = ai_install_command(&root)?;
    if dry_run {
        mgc_ui::info(&format!("[dry-run] {} {}", tool, args.join(" ")));
        return Ok(());
    }
    // Legacy uv.lock/requirements lane: the owner table cell is Native
    // (for the pyproject lane above), so the gate cannot own this path —
    // enforce the delegated contract HERE, with the gate's exact errors:
    // explicit opt-in required, flag==process (pip~pip3 alias honored),
    // loud compat announce. No flag → the same requires-compat refusal.
    // (Lane legacy: tự bắt compat tường minh, lỗi y như gate.)
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    const LEGACY_TOOLS: &[&str] = &["uv", "pip", "pip3"];
    match &compat {
        crate::commands::compat::CompatMode::Native => {
            return Err(crate::error::dep_gate_requires_compat(
                "ai",
                "install",
                Some("python"),
                LEGACY_TOOLS,
            ));
        }
        crate::commands::compat::CompatMode::Explicit(wanted) => {
            if !LEGACY_TOOLS.contains(&tool) {
                return Err(crate::error::dep_gate_lane_tool_not_owned(
                    "ai",
                    "install",
                    Some("python"),
                    tool,
                    LEGACY_TOOLS,
                ));
            }
            let same = tool == wanted.as_str()
                || matches!((tool, wanted.as_str()), ("pip", "pip3") | ("pip3", "pip"));
            if !same {
                return Err(crate::error::dep_gate_tool_mismatch(
                    "ai",
                    "install",
                    Some("python"),
                    wanted,
                    tool,
                    LEGACY_TOOLS,
                ));
            }
            mgc_ui::warning(&format!(
                "COMPATIBILITY MODE: `ai` install delegates to toolchain '{tool}' — this is NOT the native MagiCore engine path and is excluded from native-support claims."
            ));
        }
    }
    // Shared store (B-series, 2026-09-12): uv/pip caches point INSIDE
    // the mgc store (~/.magicore/store/pypi) — every ai/python project
    // on this machine reuses the same wheel/sdist bytes (one download,
    // many projects; identical root to the lib/python lane).
    // Store chia sẻ (B-series): cache uv/pip trỏ VÀO store mgc
    // (~/.magicore/store/pypi) — mọi project ai/python trên máy dùng
    // lại cùng byte wheel/sdist (tải một lần, nhiều project; cùng gốc
    // với lane lib/python).
    let shared_env = shared::shared_pypi_store_env()?;
    shared::ai_run_tool_with_env(&root, tool, &args, shared_env)?;
    Ok(())
}

// DELEGATED: uv sync / pip install run for real — mgc orchestrates only,
// it does not own this dependency lifecycle.
// (DELEGATED: uv sync / pip install chạy thật — mgc chỉ điều phối, không
// sở hữu lifecycle dependency này.)
fn ai_install_command(root: &std::path::Path) -> Result<(&'static str, Vec<String>)> {
    // Tool selection by lock file ONLY — no process spawning for probing.
    // (Chọn tool theo lock file DUY NHẤT — không spawn process để probe.)
    if root.join("uv.lock").exists() {
        return Ok(("uv", vec!["sync".to_string()]));
    }
    if root.join("requirements.lock").exists() {
        return Ok(("pip", pip_requirements_args("requirements.lock")));
    }
    if root.join("pyproject.toml").exists() {
        return Ok(("uv", vec!["sync".to_string()]));
    }
    if root.join("requirements.txt").exists() {
        return Ok(("pip", pip_requirements_args("requirements.txt")));
    }
    Err(crate::error::ai_no_lockfile())
}

fn pip_requirements_args(file: &str) -> Vec<String> {
    vec!["install".to_string(), "-r".to_string(), file.to_string()]
}

/// Native ai/python install through the shared PyPI engine (the SAME
/// pipeline lib/python uses): parse pyproject → resolve → verified
/// fetch/unpack → mgc.lock. The gate runs with Native expectation (no
/// toolchain); uv.lock-only projects never reach here (legacy lane).
/// (Install ai/python native qua engine PyPI chung.)
async fn install_python_native(
    root: &std::path::Path,
    dry_run: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    if dry_run {
        mgc_ui::info(
            "[dry-run] would run: native ai/python install (resolve PyPI, write mgc.lock)",
        );
        return Ok(());
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "ai",
            Some(crate::commands::dep_gate::eco::PYTHON),
            None,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
        .map_err(|e| anyhow::anyhow!("ai native install needs the lib PyPI engine: {e}"))?;
    crate::commands::core::shared::install_with_adapter(
        &*adapter,
        root,
        "mgc add",
        false,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
