//! `mgc add app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    gate_react_native, language, manifest_hint, project_root, run_tool, tool_command,
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

/// Convert the app CLI's Flutter constraint syntax into the shared native
/// dependency-spec syntax (`name@range`).
/// Chuyển cú pháp constraint Flutter của CLI sang cú pháp dependency native.
fn flutter_native_specs(packages: &[String], version: Option<&str>) -> Result<Vec<String>> {
    let mut specs = Vec::new();
    for raw in packages {
        for package in raw.split_whitespace() {
            let (name, inline_range) = match package.split_once(':') {
                Some((name, range)) if !name.is_empty() && !range.is_empty() => (name, Some(range)),
                Some(_) => return Err(crate::error::add_app_usage()),
                None => (package, None),
            };
            if inline_range.is_some()
                && let Some(version) = version
            {
                return Err(crate::error::add_version_conflict(package, version));
            }
            let range = inline_range.or(version);
            specs.push(match range {
                Some(range) => format!("{name}@{range}"),
                None => name.to_string(),
            });
        }
    }
    Ok(specs)
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
    // tool_command is PURE (string building, zero spawn). Order: React
    // Native gates first (no runner for any verb — P0#2), then gate with
    // the resolved tool (None when the verb has no command — the exact
    // table cell answers Unsupported for swift/kotlin/objc verbs without
    // runners). The manifest hint below is defensive fallback.
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    gate_react_native(&root, lang, crate::commands::dep_gate::DepOp::Add, &compat)?;
    if lang == mgc_app_adapter::AppLanguage::Flutter {
        let adapter = mgc_app_adapter::adapter_for(&root)
            .ok_or_else(crate::error::app_project_not_detected)?;
        crate::commands::dep_gate::gate_native_adapter(
            &crate::commands::dep_gate::DepContext::new(
                "app",
                Some(lang.ecosystem()),
                None,
                None,
                crate::commands::dep_gate::DepOp::Add,
            ),
            &adapter,
            &compat,
        )?;
        return crate::commands::core::shared::add(
            &adapter,
            &root,
            flutter_native_specs(&packages, _version.as_deref())?,
            None,
            _dev,
            _exact,
            _optional,
            _peer,
            _no_save,
            true,
            _global,
        )
        .await;
    }
    let cmd_opt = tool_command(lang, "add");
    // C0 ownership firewall (T0.3): single control path.
    // (Tường lửa C0: đường điều khiển duy nhất.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::Add,
        ),
        cmd_opt.as_ref().map(|c| c.tool.as_str()),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let Some(mut cmd) = cmd_opt else {
        return Err(manifest_hint(lang, "add"));
    };
    cmd.args
        .extend(apply_version_pin(lang, &packages, _version.as_deref())?);
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
