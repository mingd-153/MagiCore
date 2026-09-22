//! `mgc update` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

// DELEGATED: the args feed a REAL uv/pip run (ai_run_tool) — mgc
// orchestrates only, it does not own this dependency lifecycle.
// (DELEGATED: args này nuôi lệnh uv/pip THẬT (ai_run_tool) — mgc chỉ điều
// phối, không sở hữu lifecycle dependency này.)
fn update_args(packages: &[String], tool: &str) -> Vec<String> {
    if packages.is_empty() {
        return if tool == "uv" {
            vec!["lock".to_string(), "--upgrade".to_string()]
        } else {
            vec!["list".to_string(), "--outdated".to_string()]
        };
    }
    if tool == "uv" {
        let mut args = vec!["lock".to_string()];
        for p in packages.iter().flat_map(|p| p.split_whitespace()) {
            args.push("--upgrade-package".to_string());
            args.push(p.to_string());
        }
        args
    } else {
        let mut args = vec!["install".to_string(), "--upgrade".to_string()];
        args.extend(
            packages
                .iter()
                .flat_map(|p| p.split_whitespace().map(String::from)),
        );
        args
    }
}

// DELEGATED: uv lock/sync and pip flows run for real — mgc orchestrates
// only, it does not own this dependency lifecycle.
// (DELEGATED: uv lock/sync và luồng pip chạy thật — mgc chỉ điều phối,
// không sở hữu lifecycle dependency này.)
pub async fn update(
    packages: Vec<String>,
    install: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::ai_project_root()?;
    // Native lane (mgc.lock, no uv.lock): a pyproject.toml means updates
    // resolve through the NATIVE PyPI pipeline (resolve-latest + mgc-side
    // edit + install tail) — zero uv/pip spawn. uv.lock/requirements-only
    // projects keep the legacy delegated lane below (explicit compat).
    // (Lane native: pyproject → pipeline PyPI native.)
    if root.join("pyproject.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "ai",
                Some(crate::commands::dep_gate::eco::PYTHON),
                None,
                None,
                crate::commands::dep_gate::DepOp::Update,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("ai native update needs the lib PyPI engine: {e}"))?;
        // Mutation gateway: direct native_update callers hold the writer
        // lock themselves (shared::update is bypassed here by design).
        // (Gọi native trực tiếp thì tự giữ lock writer.)
        let write_lock = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
            &root,
            crate::commands::core::shared::writer_lock_timeout(&root),
        )
        .map_err(|e| anyhow::anyhow!("update cannot acquire the project writer lock: {e}"))?;
        return crate::commands::core::shared::native_update(
            &*adapter,
            &root,
            packages,
            install,
            &write_lock,
        )
        .await;
    }
    let tool = shared::ai_pick_tool(&root);
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): single control path — uv/pip spawn
    // only behind an explicit compat opt-in.
    // (Tường lửa C0: đường điều khiển duy nhất — chỉ spawn uv/pip khi có
    // compat tường minh.)
    // Legacy uv.lock/requirements lane: the owner table cell is Native
    // (for the pyproject lane above), so the gate cannot own this path —
    // enforce the delegated contract HERE with the gate's exact errors.
    // (Lane legacy: tự bắt compat tường minh, lỗi y như gate.)
    const LEGACY_TOOLS: &[&str] = &["uv", "pip", "pip3"];
    match &compat {
        crate::commands::compat::CompatMode::Native => {
            return Err(crate::error::dep_gate_requires_compat(
                "ai",
                "update",
                Some("python"),
                LEGACY_TOOLS,
            ));
        }
        crate::commands::compat::CompatMode::Explicit(wanted) => {
            if !LEGACY_TOOLS.contains(&tool) {
                return Err(crate::error::dep_gate_lane_tool_not_owned(
                    "ai",
                    "update",
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
                    "update",
                    Some("python"),
                    wanted,
                    tool,
                    LEGACY_TOOLS,
                ));
            }
            mgc_ui::warning(&format!(
                "COMPATIBILITY MODE: `ai` update delegates to toolchain '{tool}' — this is NOT the native MagiCore engine path and is excluded from native-support claims."
            ));
        }
    }
    let args = update_args(&packages, tool);
    shared::ai_run_tool(&root, tool, &args)?;
    if packages.is_empty() {
        if tool == "uv" {
            if install {
                shared::ai_run_tool(&root, tool, &["sync".to_string()])?;
            }
        } else {
            mgc_ui::info(
                "Run `pip list --outdated` to see newer versions — pip does not auto-upgrade the lock.",
            );
        }
        return Ok(());
    }
    if tool == "uv" {
        if install {
            shared::ai_run_tool(&root, tool, &["sync".to_string()])?;
        }
    } else {
        // pip: cập nhật lock sau khi upgrade
        let locked = shared::ai_run_tool_capture(&root, tool, &["freeze".to_string()])?;
        std::fs::write(root.join("requirements.lock"), locked)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
