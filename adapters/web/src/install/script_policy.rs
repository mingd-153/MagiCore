//! Lifecycle script policy helpers — lifecycle trust and environment gates.
//! Helper chính sách lifecycle — gom cổng env và trust để install orchestrator gọn hơn.

use mgc_store::{Database, Layout};
use mgc_types::MgError;

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
