use anyhow::Result;
#[cfg(feature = "web")]
use serde::Serialize;

#[cfg(feature = "web")]
use crate::context::ProjectContext;
#[cfg(feature = "web")]
use mg_ui::{info, success};

/// mg outdated — check for outdated packages
pub async fn run(core: Option<&str>, json: bool) -> Result<()> {
    outdated_web(core, json).await
}

async fn outdated_web(core: Option<&str>, json: bool) -> Result<()> {
    #[cfg(not(feature = "web"))]
    {
        let _ = (core, json);
        anyhow::bail!("outdated currently requires the web core registry adapter, which is not included in this build");
    }

    #[cfg(feature = "web")]
    {
        let ctx = ProjectContext::load_with_core(core)?;
        let adapter = ctx.adapter();

        let manifest = adapter.parse_manifest(ctx.root()).await?;
        let all_deps: Vec<_> = manifest.all_dependencies().collect();

        if all_deps.is_empty() {
            if json {
                println!("[]");
            } else {
                info("No dependencies to check");
            }
            return Ok(());
        }

        if !json {
            info(&format!(
                "Checking {} dependencies for updates...",
                all_deps.len()
            ));
        }

        let registry =
            mg_web_adapter::native::npm_registry::NpmRegistry::new("https://registry.npmjs.org");

        let mut outdated_pkgs: Vec<OutdatedPkg> = Vec::new();

        for dep in all_deps {
            if let Ok(meta) = registry.fetch_metadata(dep.name.as_str()).await {
                let latest = meta.dist_tags.get("latest");
                if let Some(latest_ver) = latest {
                    if let Ok(lv) = mg_types::Version::parse(latest_ver) {
                        if !dep.range.matches(&lv) {
                            outdated_pkgs.push(OutdatedPkg {
                                name: dep.name.to_string(),
                                current: dep.range.to_string(),
                                latest: latest_ver.to_string(),
                                major: lv.major,
                                minor: lv.minor,
                                patch: lv.patch,
                            });
                        }
                    }
                }
            }
        }

        if json {
            println!("{}", serde_json::to_string_pretty(&outdated_pkgs)?);
            return Ok(());
        }

        if outdated_pkgs.is_empty() {
            success("All packages are up to date!");
        } else {
            for pkg in &outdated_pkgs {
                let severity = severity_label(&pkg.current, pkg.major);
                info(&format!(
                    "  {}: {} → {} ({})",
                    pkg.name, pkg.current, pkg.latest, severity
                ));
            }
            info(&format!(
                "{} package(s) outdated. Run 'mg update' to update.",
                outdated_pkgs.len(),
            ));
        }

        Ok(())
    }
}

#[cfg(feature = "web")]
fn severity_label(current: &str, latest_major: u64) -> &'static str {
    let cur_major = current
        .trim_start_matches('^')
        .trim_start_matches('~')
        .split('.')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if latest_major > cur_major {
        "major"
    } else if latest_major == cur_major {
        "minor"
    } else {
        "patch"
    }
}

#[cfg(feature = "web")]
#[derive(Debug, Serialize)]
struct OutdatedPkg {
    name: String,
    current: String,
    latest: String,
    major: u64,
    minor: u64,
    patch: u64,
}
