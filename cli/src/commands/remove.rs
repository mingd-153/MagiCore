use anyhow::Result;
use mg_ui::{success, info, style_cmd};
use crate::context::ProjectContext;

/// mg remove — remove a dependency from the project
pub async fn run(package: String, core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    let name = mg_types::PackageName::new(&package)?;
    info(&format!("Removing {} from {}...", package, ctx.config.name));

    adapter.remove(ctx.root(), &name).await?;

    success(&format!("Removed {}", package));
    info(&format!("Run '{}' to update lockfile", style_cmd("mg install")));

    Ok(())
}
