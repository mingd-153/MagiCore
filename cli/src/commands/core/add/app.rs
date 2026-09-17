//! `mgc add app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    language, manifest_hint, project_root, run_tool, tool_command,
};

/// Apply a `--version` pin to provider-tool args: Flutter pub accepts
/// `pkg:constraint` (verified against the Dart pub reference); every
/// other app language has no verified pin syntax and fails loudly.
/// A package already carrying `:` plus `--version` is ambiguous — failed
/// loudly, never merged silently.
/// (Áp pin `--version`: Flutter pub chấp nhận `pkg:constraint`; ngôn ngữ
/// app khác fail rõ.)
pub fn apply_version_pin(
    lang: mgc_app_adapter::AppLanguage,
    packages: &[String],
    version: Option<&str>,
) -> Result<Vec<String>> {
    let flat: Vec<String> = packages
        .iter()
        .flat_map(|p| p.split_whitespace().map(String::from))
        .collect();
    let Some(pinned) = version else {
        return Ok(flat);
    };
    if pinned.trim().is_empty() {
        return Err(crate::error::add_version_unsupported("app", "(empty)"));
    }
    if lang != mgc_app_adapter::AppLanguage::Flutter {
        return Err(crate::error::add_version_unsupported("app", pinned));
    }
    flat.into_iter()
        .map(|p| {
            if p.contains(':') {
                Err(crate::error::add_version_conflict(&p, pinned))
            } else {
                Ok(format!("{p}:{pinned}"))
            }
        })
        .collect()
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
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        return Err(crate::error::add_app_usage());
    }
    let Some(mut cmd) = tool_command(lang, "add") else {
        return Err(manifest_hint(lang, "add"));
    };
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): single control path.
    // (Tường lửa C0: đường điều khiển duy nhất.)
    crate::commands::dep_gate::gate(
        "app",
        crate::commands::dep_gate::DepOp::Add,
        Some(cmd.tool.as_str()),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    cmd.args
        .extend(apply_version_pin(lang, &packages, _version.as_deref())?);
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
