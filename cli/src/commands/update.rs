use anyhow::Result;
use mg_ui::{success, info, create_spinner, style_cmd};
use crate::context::ProjectContext;

/// mg update — update packages to latest versions
pub async fn run(packages: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    if packages.is_empty() {
        info("Checking for outdated packages...");
        let spinner = create_spinner("  Resolving latest versions...");
        let updated = adapter.update(ctx.root(), None).await?;
        spinner.finish_and_clear();

        if updated.is_empty() {
            info("All packages are up to date");
        } else {
            for pkg in &updated {
                info(&format!("  {}: {} → {}", pkg.name, pkg.from_version, pkg.to_version));
            }
            success(&format!("Updated {} package(s)", updated.len()));
            info(&format!("Run '{}' to install updates", style_cmd("mg install")));
        }
    } else {
        for name in &packages {
            let pn = mg_types::PackageName::new(name)?;
            let spinner = create_spinner(&format!("  Updating {}...", name));
            let updated = adapter.update(ctx.root(), Some(&pn)).await?;
            spinner.finish_and_clear();

            for pkg in &updated {
                info(&format!("  {}: {} → {}", pkg.name, pkg.from_version, pkg.to_version));
            }
        }
        success("Update complete");
    }

    Ok(())
}
