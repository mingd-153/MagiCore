use anyhow::Result;

#[cfg(feature = "web")]
use crate::context::ProjectContext;
#[cfg(feature = "web")]
use mg_ui::{info, success};

/// mg outdated — check for outdated packages
pub async fn run(core: Option<&str>) -> Result<()> {
    #[cfg(feature = "web")]
    return outdated_web(core).await;
    #[cfg(not(feature = "web"))]
    {
        let _ = core;
        anyhow::bail!("'mg outdated' is not available in this build (requires web core)")
    }
}

#[cfg(feature = "web")]
async fn outdated_web(core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    let manifest = adapter.parse_manifest(ctx.root()).await?;
    let all_deps: Vec<_> = manifest.all_dependencies().collect();

    if all_deps.is_empty() {
        info("No dependencies to check");
        return Ok(());
    }

    info(&format!(
        "Checking {} dependencies for updates...",
        all_deps.len()
    ));

    let registry =
        mg_web_adapter::native::npm_registry::NpmRegistry::new("https://registry.npmjs.org");
    let mut outdated = 0;

    for dep in all_deps {
        match registry.fetch_metadata(dep.name.as_str()).await {
            Ok(meta) => {
                let latest = meta.dist_tags.get("latest");
                if let Some(latest_ver) = latest {
                    if let Ok(lv) = mg_types::Version::parse(latest_ver) {
                        if !dep.range.matches(&lv) {
                            info(&format!(
                                "  {}: {} → {} (latest)",
                                dep.name, dep.range, latest_ver
                            ));
                            outdated += 1;
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }

    if outdated == 0 {
        success("All packages are up to date!");
    } else {
        info(&format!(
            "{} package(s) outdated. Run '{}' to update.",
            outdated, "mg update"
        ));
    }

    Ok(())
}
