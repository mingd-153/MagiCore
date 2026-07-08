use anyhow::Result;
use mg_types::adapter::AddOptions;
use mg_ui::{success, info, create_spinner, style_cmd};
use crate::context::ProjectContext;

/// mg add — add a dependency to the project
pub async fn run(
    package: String, version: Option<String>, dev: bool, exact: bool,
    optional: bool, peer: bool, no_save: bool, global: bool, core: Option<&str>,
) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    let spec = mg_types::DependencySpec::parse(&package)?;
    let name = spec.name;
    let range = if let Some(v) = version {
        Some(mg_types::VersionRange::parse(&v)?)
    } else if spec.range.is_star() {
        None
    } else {
        Some(spec.range)
    };

    let group = if peer { "peerDependencies" } else if optional { "optionalDependencies" } else if dev { "devDependencies" } else { "dependencies" };
    mg_ui::info(&format!("Adding {} to {} ({})...", package, ctx.config.name, group));

    let spinner = mg_ui::create_spinner(&format!("  Resolving {}...", package));
    let opts = AddOptions { dev, optional, peer, exact, no_save, global };
    let pkg_id = adapter.add(ctx.root(), &name, range.as_ref(), opts).await?;
    spinner.finish_and_clear();

    if !no_save {
        mg_ui::info(&format!("  {}@{} added to {}", pkg_id.name_str(), pkg_id.version(), group));
        mg_ui::success(&format!("Added {}", package));
        mg_ui::info(&format!("Run '{}' to install", mg_ui::style_cmd("mg install")));
    } else {
        mg_ui::info(&format!("  {}@{} resolved (--no-save, manifest unchanged)", pkg_id.name_str(), pkg_id.version()));
    }

    Ok(())
}
