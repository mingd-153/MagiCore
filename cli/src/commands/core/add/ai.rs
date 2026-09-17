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
    let tool = shared::ai_pick_tool(&root);
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): single control path — uv/pip spawn
    // only behind an explicit compat opt-in.
    // (Tường lửa C0: đường điều khiển duy nhất — chỉ spawn uv/pip khi có
    // compat tường minh.)
    crate::commands::dep_gate::gate(
        "ai",
        crate::commands::dep_gate::DepOp::Add,
        Some(tool),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    // --version pins via PEP 508 `==`; conflicting inline specs fail
    // loudly inside add_args (never merged silently).
    let args = add_args(&packages, tool, _version.as_deref())?;
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
