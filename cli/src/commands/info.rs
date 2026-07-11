use anyhow::Result;

#[cfg(feature = "web")]
use mg_ui::{info, warning};

/// mg info — show package information from registry
pub async fn run(package: String) -> Result<()> {
    #[cfg(feature = "web")]
    return info_web(package).await;
    #[cfg(not(feature = "web"))]
    {
        let _ = package;
        anyhow::bail!("'mg info' is not available in this build (requires web core)")
    }
}

#[cfg(feature = "web")]
async fn info_web(package: String) -> Result<()> {
    let registry =
        mg_web_adapter::native::npm_registry::NpmRegistry::new("https://registry.npmjs.org");

    info(&format!("Fetching info for {}...", package));

    match registry.fetch_metadata(&package).await {
        Ok(meta) => {
            println!();
            info(&format!("Package: {}", meta.name));
            if let Some(desc) = meta.description {
                info(&format!("Description: {}", desc));
            }
            info(&format!("Versions: {}", meta.versions.len()));

            if !meta.dist_tags.is_empty() {
                println!();
                for (tag, ver) in &meta.dist_tags {
                    info(&format!("  {}: {}", tag, ver));
                }
            }

            let mut sorted: Vec<_> = meta.versions.keys().collect();
            sorted.sort();
            let recent: Vec<_> = sorted.iter().rev().take(5).collect();
            if !recent.is_empty() {
                println!();
                info("Recent versions:");
                for v in recent {
                    info(&format!("  {}", v));
                }
            }
        }
        Err(e) => {
            warning(&format!("Could not fetch package info: {}", e));
        }
    }

    Ok(())
}
