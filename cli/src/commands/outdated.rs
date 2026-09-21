use anyhow::Result;
#[cfg(feature = "web")]
use serde::Serialize;

#[cfg(feature = "web")]
use crate::commands::web_registry_config::web_registry_url;
#[cfg(feature = "web")]
use crate::context::ProjectContext;
#[cfg(feature = "web")]
use mgc_ui::{info, success};

/// mgc outdated — check for outdated packages
pub async fn run(core: Option<&str>, json: bool) -> Result<()> {
    outdated_web(core, json).await
}

async fn outdated_web(core: Option<&str>, json: bool) -> Result<()> {
    #[cfg(not(feature = "web"))]
    {
        let _ = (core, json);
        return Err(crate::error::outdated_no_web_adapter());
    }

    #[cfg(feature = "web")]
    {
        let ctx = ProjectContext::load_with_core(core)?;
        let adapter = ctx.adapter();
        // P0/F6: arm the age gate from THIS operation's project (broken
        // config fails here, not silently unfiltered).
        adapter.arm_age_gate_for(ctx.root())?;
        let policy = mgc_web_adapter::WebAdapter::load_age_policy_for(ctx.root())?;

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

        let registry = mgc_web_adapter::native::npm_registry::NpmRegistry::new(&web_registry_url());
        registry.set_age_gate_armed(policy.is_some_and(|p| p.cutoff_hours > 0));

        let mut outdated_pkgs: Vec<OutdatedPkg> = Vec::new();
        let mut checked = 0usize;
        let mut failed: Vec<String> = Vec::new();

        for dep in all_deps {
            match registry.fetch_metadata(dep.name.as_str()).await {
                Ok(meta) => {
                    checked += 1;
                    // Resolve the reported "latest" through the age gate:
                    // suggesting a quarantined newest as the update target
                    // would route users around the policy. When the
                    // registry newest is excluded, fall back to the newest
                    // ELIGIBLE version (or skip when none qualifies).
                    // (Latest báo cáo cũng qua cổng tuổi.)
                    let latest = meta.dist_tags.get("latest");
                    let latest_ver: Option<String> = match latest {
                        Some(candidate)
                            if mgc_web_adapter::provider::check_pinned_version(
                                &dep.name, &meta, candidate, policy,
                            )
                            .is_ok() =>
                        {
                            Some(candidate.clone())
                        }
                        _ => match mgc_web_adapter::provider::eligible_versions(
                            &dep.name, &meta, policy,
                        ) {
                            Ok(eligible) => eligible.into_iter().max().map(|v| v.to_string()),
                            Err(_) => None,
                        },
                    };
                    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
                    if let Some(latest_ver) = latest_ver
                        && let Ok(lv) = mgc_types::Version::parse(&latest_ver)
                        && !dep.range.matches(&lv)
                    {
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
                Err(_) => {
                    failed.push(dep.name.to_string());
                }
            }
        }

        // Fail closed on total fetch failure: reporting "up to date" when
        // NOTHING was checked is a lie (audit finding). Partial failures
        // warn but still report what was checked.
        // (Fetch fail hết → lỗi cứng, không báo "up to date" láo.)
        if checked == 0 {
            return Err(crate::error::outdated_no_registry_response(
                failed.join(", "),
            ));
        }
        if !failed.is_empty() {
            mgc_ui::warning(&format!(
                "could not check {} package(s) (registry unreachable): {}",
                failed.len(),
                failed.join(", ")
            ));
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
                "{} package(s) outdated. Run 'mgc update' to update.",
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
