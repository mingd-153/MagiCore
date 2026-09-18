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
use crate::native::npm_registry::{PackageMetadata, check_publish_age};

const DEFAULT_QUARANTINE_SECS: u64 = 86400;

pub fn enforce_resolution_supply_chain_guards(
    resolutions: &[Resolution],
    metadata: &HashMap<String, Arc<PackageMetadata>>,
    manifest: &mgc_types::Manifest,
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
        enforce_no_downgrade(resolutions, manifest)?;
    }

    Ok(())
}

fn configured_store_min_age() -> Option<u64> {
    // Try reading from mg.toml [security] first — Đọc từ mg.toml [security] trước
    let cwd = std::env::current_dir().ok()?;

    // Read mg.toml config — Đọc config mg.toml
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(Some(project)) = ProjectConfig::load(&cwd)
        && let Some(security) = &project.security
        && let Some(min_age) = security.min_age_for_ecosystem("web")
    {
        return Some(min_age);
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
            "WARN: [magicore] WARNING: MAGICORE_ALLOW_UNTRUSTED=1 — supply-chain guards\n   \
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

fn enforce_no_downgrade(
    resolutions: &[Resolution],
    manifest: &mgc_types::Manifest,
) -> MgResult<()> {
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(());
    };
    // Baseline = the PREVIOUS top-level pins from the project lockfile
    // (only when that lock satisfies the current manifest — a stale or
    // corrupt lock is not a baseline). The installed-versions DB is the
    // WRONG baseline: it accumulates every nested transitive line, so a
    // tree that legitimately contains two major lines (vue-router 4
    // direct + vue-router 5 nested under a dependent) false-positives.
    // With no satisfying lock there is no previous top level — nothing
    // to regress from, so the guard passes (fresh installs cannot
    // downgrade; registry freshness is the quarantine's job).
    // (Baseline = pin top-level cũ trong lockfile, không phải max DB.)
    let previous_top: std::collections::HashMap<String, mgc_types::Version> = (|| {
        let lockfile = crate::lockfile::read_web_lockfile_checked(&cwd).ok()??;
        if !crate::lockfile::lockfile_satisfies_manifest(&lockfile, manifest) {
            return None;
        }
        let mut map = std::collections::HashMap::new();
        for dep in manifest.all_dependencies() {
            if let Some(lp) = lockfile
                .packages
                .iter()
                .find(|p| p.name == dep.name.as_str())
                && let Ok(version) = mgc_types::Version::parse(&lp.version)
            {
                map.insert(dep.name.as_str().to_string(), version);
            }
        }
        Some(map)
    })()
    .unwrap_or_default();
    if previous_top.is_empty() {
        return Ok(());
    }
    // Per DIRECT dep: the version that will link at top level is the MAX
    // resolution satisfying the manifest range (npm semantics). Only THAT
    // selection is compared against the previous top-level pin — a lower
    // top selection is a genuine top-level regression and fails closed.
    // (Mỗi dep trực tiếp: chỉ version max thỏa range mới so với pin cũ.)
    for dep in manifest.all_dependencies() {
        let name = dep.name.as_str();
        let Some(baseline) = previous_top.get(name) else {
            continue;
        };
        let top = resolutions
            .iter()
            .filter(|r| {
                r.package_id.name_str() == name && dep.range.matches(r.package_id.version())
            })
            .max_by(|a, b| a.package_id.version().cmp(b.package_id.version()));
        let Some(top) = top else {
            return Err(mgc_types::MgError::Other(format!(
                "🚨 SECURITY: no resolved version of direct dep '{name}' satisfies '{}' — refusing to link an out-of-range top level (fail-closed)",
                dep.range.as_str()
            )));
        };
        let new_v = top.package_id.version();
        if *new_v < *baseline {
            return Err(mgc_types::MgError::Other(format!(
                "🚨 SECURITY: Downgrade blocked for '{name}' — previous top-level {baseline}, new top-level selection {new_v}.\n   \
                 Use MAGICORE_ALLOW_UNTRUSTED=1 to override."
            )));
        }
    }
    Ok(())
}
