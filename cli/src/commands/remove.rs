use crate::context::ProjectContext;
use anyhow::Result;
use mgc_ui::{info, style_cmd, success};

/// mgc remove — remove a dependency from the project
#[allow(dead_code)]
pub async fn run(package: String, core: Option<&str>) -> Result<()> {
    run_many(vec![package], core).await
}

#[allow(dead_code)]
pub async fn run_many(packages: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();
    info(&format!(
        "Removing {} package(s) from {}...",
        packages.len(),
        ctx.config.name
    ));

    for package in &packages {
        let name = mgc_types::PackageName::new(package)?;
        adapter.remove(ctx.root(), &name).await?;
        success(&format!("Removed {}", package));
    }

    info(&format!(
        "Run '{}' to update lockfile",
        style_cmd("mgc install")
    ));

    Ok(())
}
