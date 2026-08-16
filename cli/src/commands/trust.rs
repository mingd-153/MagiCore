//! mg trust — approve/deny lifecycle scripts per package (T5 security gate).
//! (Quản lý allowlist trust cho lifecycle scripts: approved → chạy,
//!  denied → chặn, unlisted → fail-closed.)
//!
//! State: bảng `trust_policy` trong store.db (project-local SQLite, mg-store).

use anyhow::{bail, Result};
use clap::Subcommand;
use mg_config::project::ProjectConfig;
use mg_store::database::Database;
use std::path::Path;

#[derive(Subcommand, Debug, Clone)]
pub enum TrustCmd {
    /// List trust policies
    Ls,
    /// Approve lifecycle scripts for a package (bare name = all versions)
    Approve { package: String },
    /// Deny lifecycle scripts for a package (bare name = all versions)
    Deny { package: String },
    /// Remove policies for packages no longer installed
    Prune,
    /// Show/set the min-release-age quarantine (seconds) for lifecycle installs.
    /// (0 = disabled; omit `secs` to show the current value; default 86400)
    Policy { secs: Option<u64> },
}

/// Project-local store DB — same path the web adapter uses (`layout.db_path()`).
fn store_db_for(project_root: &Path) -> Result<std::path::PathBuf> {
    let store_root = project_root.join(".megagate").join("cache").join("web");
    Ok(store_root.join("store.db"))
}

/// Normalize a trust key: `name@version` (parsed → canonical) or bare `name`
/// (scoped hoặc không → covers all versions).
fn package_key(spec: &str) -> Result<String> {
    if spec.is_empty() {
        bail!("empty package name");
    }
    if let Ok(id) = mg_types::PackageId::parse(spec) {
        Ok(id.to_string())
    } else {
        Ok(spec.to_string())
    }
}

fn ago(updated_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = now.saturating_sub(updated_at);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// mg trust — manage the lifecycle-script allowlist.
pub async fn run(cmd: TrustCmd) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root = ProjectConfig::find_project_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!("Project root not found — run mg init"))?;
    let db_path = store_db_for(&project_root)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = Database::open(&db_path)?;

    match cmd {
        TrustCmd::Ls => {
            let policies = db.list_trust_policies()?;
            if policies.is_empty() {
                println!("No trust policies configured");
                println!("  mg trust approve <pkg[@version]> — allow lifecycle scripts");
                println!("  mg trust deny    <pkg[@version]> — block lifecycle scripts");
                return Ok(());
            }
            println!("Trust policies (script allowlist):");
            for (pkg_id, policy, updated_at) in &policies {
                println!("  {pkg_id:<30} {policy:<8} (updated {})", ago(*updated_at));
            }
            println!();
            println!("  approved → lifecycle scripts run on install");
            println!("  denied   → lifecycle scripts never run");
            println!("  unlisted → fail-closed, scripts do NOT run");
            Ok(())
        }
        TrustCmd::Approve { package } => {
            let key = package_key(&package)?;
            db.upsert_trust_policy(&key, "approved")?;
            println!("Approved lifecycle scripts for {package}");
            Ok(())
        }
        TrustCmd::Deny { package } => {
            let key = package_key(&package)?;
            db.upsert_trust_policy(&key, "denied")?;
            println!("Denied lifecycle scripts for {package}");
            Ok(())
        }
        TrustCmd::Prune => {
            let pruned = db.prune_trust_policies()?;
            println!(
                "Pruned {pruned} trust polic{} for packages no longer installed",
                if pruned == 1 { "y" } else { "ies" }
            );
            Ok(())
        }
        TrustCmd::Policy { secs } => match secs {
            None => {
                match db.release_policy("web")? {
                    Some(v) => {
                        println!("Min-release-age quarantine for web: {}s ({}h)", v, v / 3600)
                    }
                    None => println!(
                        "Min-release-age quarantine for web: not set (default 86400s / 24h)"
                    ),
                }
                Ok(())
            }
            Some(secs) => {
                db.upsert_release_policy("web", secs)?;
                if secs == 0 {
                    println!("Disabled min-release-age quarantine for web");
                } else {
                    println!(
                        "Min-release-age quarantine for web set to {}s ({}h).\n  \
                         Enforced on the next resolve — use MEGAGATE_ALLOW_UNTRUSTED=1 to skip.",
                        secs,
                        secs / 3600
                    );
                }
                Ok(())
            }
        },
    }
}
