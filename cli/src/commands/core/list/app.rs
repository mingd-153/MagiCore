//! `mgc list app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    let Some(cmd) = tool_command(lang, "list") else {
        return Err(manifest_hint(lang, "list"));
    };
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
