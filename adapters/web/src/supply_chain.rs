// Supply-chain guards for core-web resolution — quarantine and downgrade checks.
// Guard chuỗi cung ứng cho core-web — tách policy khỏi flow resolve chính.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

use mgc_config::ProjectConfig;
use mgc_resolver::solver::Resolution;
use mgc_store::{Database, Layout};
use mgc_types::MgResult;

use crate::lockfile::project_cache_dir;
use crate::native::npm_registry::{check_publish_age, PackageMetadata};

const DEFAULT_QUARANTINE_SECS: u64 = 86400;

pub fn enforce_resolution_supply_chain_guards(
    resolutions: &[Resolution],
    metadata: &HashMap<String, Arc<PackageMetadata>>,
) -> MgResult<()> {
    let store_min_age = configured_store_min_age();
    let block_new = env_truthy("MAGICORE_SECURITY_24H_BLOCK")
        || env_truthy("MGC_AUDIT_STRICT")
        || store_min_age.is_some();
    let min_age_secs = store_min_age.unwrap_or(DEFAULT_QUARANTINE_SECS) as i64;
    let allow_untrusted = env_truthy("MAGICORE_ALLOW_UNTRUSTED");

    warn_once_if_untrusted(allow_untrusted);

    if block_new && !allow_untrusted {
        enforce_publish_age(resolutions, metadata, min_age_secs)?;
    }
    if !allow_untrusted {
        enforce_no_downgrade(resolutions)?;
    }

    Ok(())
}

fn configured_store_min_age() -> Option<u64> {
    // Try reading from mg.toml [security] first — Đọc từ mg.toml [security] trước
    let cwd = std::env::current_dir().ok()?;
    
    // Read mg.toml config — Đọc config mg.toml
    if let Ok(Some(project)) = ProjectConfig::load(&cwd) {
        if let Some(security) = &project.security {
            if let Some(min_age) = security.min_age_for_ecosystem("web") {
                return Some(min_age);
            }
        }
    }
    
    // Fallback to database release_policy — Dự phòng đọc từ database
    let layout = Layout::new(project_cache_dir(&cwd));
    Database::open(&layout.db_path())
        .ok()
        .and_then(|db| db.release_policy("web").ok().flatten())
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn warn_once_if_untrusted(allow_untrusted: bool) {
    if !allow_untrusted {
        return;
    }
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        eprintln!(
            "⚠️  [magicore] WARNING: MAGICORE_ALLOW_UNTRUSTED=1 — supply-chain guards\n   \
             (24h quarantine + no-downgrade) are BYPASSED for this process."
        );
    });
}

fn enforce_publish_age(
    resolutions: &[Resolution],
    metadata: &HashMap<String, Arc<PackageMetadata>>,
    min_age_secs: i64,
) -> MgResult<()> {
    for resolution in resolutions {
        if let Some(pkg_meta) = metadata.get(resolution.package_id.name_str()) {
            let version = resolution.package_id.version().to_string();
            if let Err(msg) = check_publish_age(pkg_meta, &version, min_age_secs) {
                return Err(mgc_types::MgError::Other(msg));
            }
        }
    }
    Ok(())
}

fn enforce_no_downgrade(resolutions: &[Resolution]) -> MgResult<()> {
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(());
    };
    let layout = Layout::new(project_cache_dir(&cwd));
    let Ok(db) = Database::open(&layout.db_path()) else {
        return Ok(());
    };
    for resolution in resolutions {
        let id = &resolution.package_id;
        let old = db.latest_installed_version(id.name_str()).ok().flatten();
        if let Some(old) = old {
            let new_v = id.version();
            if *new_v < old {
                return Err(mgc_types::MgError::Other(format!(
                    "🚨 SECURITY: Downgrade blocked for '{id}' — installed {old}, requested {new_v}.\n   \
                     This can regress packages in the CAS store. Use MAGICORE_ALLOW_UNTRUSTED=1 to override."
                )));
            }
        }
    }
    Ok(())
}
