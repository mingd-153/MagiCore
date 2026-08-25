//! `mgc update app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn update(packages: Vec<String>, _install: bool) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    let Some(mut cmd) = tool_command(lang, "update") else {
        return Err(manifest_hint(lang, "update"));
    };
    cmd.args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
