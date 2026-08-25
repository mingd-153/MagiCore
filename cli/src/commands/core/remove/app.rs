//! `mgc remove app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        return Err(crate::error::remove_app_usage());
    }
    let Some(mut cmd) = tool_command(lang, "remove") else {
        return Err(manifest_hint(lang, "remove"));
    };
    cmd.args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
