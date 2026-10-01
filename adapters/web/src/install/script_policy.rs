//! Lifecycle script policy helpers — lifecycle trust and environment gates.
//! Helper chính sách lifecycle — gom cổng env và trust để install orchestrator gọn hơn.

use mgc_store::{Database, Layout};
use mgc_types::{MgError, PackageId};
use std::collections::HashMap;
use std::path::PathBuf;

/// Resolved package identity paired with its materialized directory.
/// Package identity comes from MGC's graph, never package-controlled JSON.
/// Định danh lấy từ graph đã resolve, không tin trường JSON do package tự khai.
#[derive(Debug, Clone)]
pub struct LifecyclePackage {
    pub package_id: PackageId,
    pub directory: PathBuf,
}

impl LifecyclePackage {
    pub fn new(package_id: PackageId, directory: PathBuf) -> Self {
        Self {
            package_id,
            directory,
        }
    }
}

/// Read lifecycle hook presence without following package.json symlinks.
/// Malformed or unreadable metadata is an install error, never "no scripts".
/// Đọc hook không theo symlink; metadata hỏng không được coi là không có script.
pub fn manifest_has_lifecycle_scripts(package_dir: &std::path::Path) -> Result<bool, MgError> {
    Ok(crate::lifecycle::load_package_scripts(package_dir)?.has_hooks())
}

/// Resolve the single trust decision from MGC's resolved package identity.
/// Manifest `name`/`version` are intentionally excluded because archives control them.
/// Quyết định trust dùng PackageId của MGC; archive không được tự chọn khóa policy.
pub fn decide_lifecycle_scripts(
    package_id: &PackageId,
    file_policy: Option<&mgc_config::project::ScriptsPolicy>,
    trust_map: &HashMap<String, String>,
    blanket_scripts: bool,
) -> mgc_config::project::ScriptVerdict {
    let resolved_key = package_id.to_string();
    let trust_policy = trust_map
        .get(&resolved_key)
        .or_else(|| trust_map.get(package_id.name_str()))
        .map(String::as_str);
    mgc_config::project::decide_scripts(
        package_id.name_str(),
        &package_id.version().to_string(),
        file_policy,
        trust_policy,
        blanket_scripts,
    )
}

pub fn lifecycle_scripts_allowed() -> bool {
    std::env::var("MAGICORE_WEB_ALLOW_SCRIPTS")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

pub fn should_run_lifecycle_scripts(ignore_scripts: bool, allow_scripts: bool) -> bool {
    if ignore_scripts {
        return false;
    }
    allow_scripts || lifecycle_scripts_allowed()
}

/// Machine-local trust DB (deny/approve per package). A DB that cannot
/// be opened or read is a HARD error (P0-3) — an empty map here would
/// silently drop denies the user believes are active, and with
/// --allow-scripts the scripts would then RUN. Failing closed beats a
/// believed-active deny.
/// (Trust DB máy-local — không đọc được thì lỗi cứng, không map rỗng.)
pub fn load_trust_policies(
    layout: &Layout,
) -> Result<std::collections::HashMap<String, String>, MgError> {
    let db = Database::open(&layout.db_path()).map_err(|e| {
        MgError::Other(format!(
            "cannot open trust database '{}' — install aborted (a believed-active deny must never silently vanish; use --ignore-scripts to skip lifecycle scripts): {e}",
            layout.db_path().display()
        ))
    })?;
    db.list_trust_policies()
        .map(|rows| {
            rows.into_iter()
                .map(|(id, policy, _)| (id, policy))
                .collect()
        })
        .map_err(|e| {
            MgError::Other(format!(
                "cannot read trust policies — install aborted (use --ignore-scripts to skip lifecycle scripts): {e}"
            ))
        })
}

pub fn trust_allows_script(policy: Option<&str>, blanket_scripts: bool) -> bool {
    match policy {
        Some("approved") => true,
        Some("denied") => false,
        _ => blanket_scripts,
    }
}

/// Committed `[scripts]` policy from the project mgc.toml (reviewable;
/// complements the machine-local trust DB). Missing file/table/fields =
/// no opinion (None). PRESENT but BROKEN policy file = hard error
/// (must not silently drop denials the user believes are active).
/// (Policy `[scripts]` commit được từ mgc.toml project. File hỏng thì
/// lỗi cứng, không im lặng.)
pub fn load_file_scripts_policy(
    project_root: &std::path::Path,
) -> Result<Option<mgc_config::project::ScriptsPolicy>, MgError> {
    mgc_config::project::load_scripts_table(project_root)
        .map_err(|e| MgError::Other(format!("[scripts] policy parse failed: {e}")))
}
