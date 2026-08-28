use crate::context::ProjectContext;
use anyhow::Result;
use mgc_ui::info;

/// mgc list — show installed packages
pub async fn run(core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    let packages = adapter.list(ctx.root()).await?;

    if packages.is_empty() {
        info("No packages installed");
        return Ok(());
    }

    info(&format!("Packages in {}:", ctx.config.name));
    for pkg in &packages {
        let dev = if pkg.is_dev { " (dev)" } else { "" };
        info(&format!(
            "  {}@{}{}",
            pkg.id.name_str(),
            pkg.id.version(),
            dev
        ));
    }

    Ok(())
}
