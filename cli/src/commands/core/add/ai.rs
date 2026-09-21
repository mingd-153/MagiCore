//! `mgc add` ai — tách từ core/ai.rs (Phase 7 v5). 05 §5: chốt 1 tool theo lock (uv/pip).

use anyhow::Result;

use super::super::shared;

// DELEGATED: the args feed a REAL uv/pip run (ai_run_tool) — mgc
// orchestrates only, it does not own this dependency lifecycle.
// (DELEGATED: args này nuôi lệnh uv/pip THẬT (ai_run_tool) — mgc chỉ điều
// phối, không sở hữu lifecycle dependency này.)
//
// Version pins use PEP 508 `==` (accepted by both `uv add` and
// `pip install`). A package that already carries its own spec combined
// with `--version` is ambiguous — failed loudly, never merged silently.
fn add_args(packages: &[String], tool: &str, version: Option<&str>) -> Result<Vec<String>> {
    let mut args = vec![if tool == "uv" { "add" } else { "install" }.to_string()];
    let pinned: Vec<String> = match version {
        None => packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from))
            .collect(),
        Some(v) => packages
            .iter()
            .flat_map(|p| p.split_whitespace())
            .map(|p| {
                // Extras (`pkg[extra]`) combine fine with a pin
                // (`pkg[extra]==1.0`); operator/URL specs conflict.
                if p.chars()
                    .any(|c| matches!(c, '=' | '>' | '<' | '~' | '!' | '@'))
                {
                    Err(crate::error::add_version_conflict(p, v))
                } else {
                    Ok(format!("{p}=={v}"))
                }
            })
            .collect::<Result<Vec<String>>>()?,
    };
    args.extend(pinned);
    Ok(args)
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::ai_project_root()?;
    // Native lane (mgc.lock, no uv.lock): a pyproject.toml means the dep
    // books through the NATIVE PyPI pipeline (resolve-first + mgc-side
    // pyproject edit, same engine as lib/python) — zero uv/pip spawn.
    // uv.lock/requirements-only projects keep the legacy delegated path
    // below (explicit compat).
    // (Lane native: pyproject → pipeline PyPI native.)
    if root.join("pyproject.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "ai",
                Some(crate::commands::dep_gate::eco::PYTHON),
                None,
                None,
                crate::commands::dep_gate::DepOp::Add,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("ai native add needs the lib PyPI engine: {e}"))?;
        return crate::commands::core::shared::add(
            &*adapter, &root, packages, _version, _dev, _exact, _optional, _peer, _no_save, true,
            _global,
        )
        .await;
    }
    let tool = shared::ai_pick_tool(&root);
    // Legacy uv.lock/requirements lane: the owner table cell is Native
    // (for the pyproject lane above), so the gate cannot own this path —
    // enforce the delegated contract HERE with the gate's exact errors:
    // explicit opt-in required, flag==process (pip~pip3 alias honored),
    // loud compat announce.
    // (Lane legacy: tự bắt compat tường minh, lỗi y như gate.)
    const LEGACY_TOOLS: &[&str] = &["uv", "pip", "pip3"];
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    match &compat {
        crate::commands::compat::CompatMode::Native => {
            return Err(crate::error::dep_gate_requires_compat(
                "ai",
                "add",
                Some("python"),
                LEGACY_TOOLS,
            ));
        }
        crate::commands::compat::CompatMode::Explicit(wanted) => {
            if !LEGACY_TOOLS.contains(&tool) {
                return Err(crate::error::dep_gate_lane_tool_not_owned(
                    "ai",
                    "add",
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
                    "add",
                    Some("python"),
                    wanted,
                    tool,
                    LEGACY_TOOLS,
                ));
            }
            mgc_ui::warning(&format!(
                "COMPATIBILITY MODE: `ai` add delegates to toolchain '{tool}' — this is NOT the native MagiCore engine path and is excluded from native-support claims."
            ));
        }
    }
    // --version pins via PEP 508 `==`; conflicting inline specs fail
    // loudly inside add_args (never merged silently).
    let args = add_args(&packages, tool, _version.as_deref())?;
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
