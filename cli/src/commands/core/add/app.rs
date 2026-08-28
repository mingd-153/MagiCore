//! `mgc add app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    language, manifest_hint, project_root, run_tool, tool_command,
};

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
) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        return Err(crate::error::add_app_usage());
    }
    let Some(mut cmd) = tool_command(lang, "add") else {
        return Err(manifest_hint(lang, "add"));
    };
    cmd.args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
