use anyhow::Result;
use mg_ui::{success, info, create_spinner, style_cmd};
use crate::context::ProjectContext;

/// mg add — add a dependency to the project
/// Supports both:
///   mg add react@^19.0.0          ← version inline (npm-style)
///   mg add react --version ^19.0.0 ← flag override
pub async fn run(package: String, version: Option<String>, dev: bool, core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    // Parse package@version from the package string (npm-style)
    let spec = mg_types::DependencySpec::parse(&package)?;
    let name = spec.name;
    // --version flag overrides inline version; omit range if "*" (latest)
    let range = if let Some(v) = version {
        Some(mg_types::VersionRange::parse(&v)?)
    } else if spec.range.is_star() {
        None
    } else {
        Some(spec.range)
    };

    info(&format!("Adding {} to {}...", package, ctx.config.name));

    let spinner = create_spinner(&format!("  Resolving {}...", package));
    let pkg_id = adapter.add(ctx.root(), &name, range.as_ref(), dev).await?;
    spinner.finish_and_clear();

    info(&format!("  {}@{} added to {}", pkg_id.name_str(), pkg_id.version(), if dev { "devDependencies" } else { "dependencies" }));
    success(&format!("Added {}", package));
    info(&format!("Run '{}' to install", style_cmd("mg install")));

    Ok(())
}
