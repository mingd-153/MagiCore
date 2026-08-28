//! Lifecycle script policy helpers — lifecycle trust and environment gates.
//! Helper chính sách lifecycle — gom cổng env và trust để install orchestrator gọn hơn.

use mgc_store::{Database, Layout};

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

pub fn load_trust_policies(layout: &Layout) -> std::collections::HashMap<String, String> {
    Database::open(&layout.db_path())
        .and_then(|db| db.list_trust_policies())
        .map(|rows| {
            rows.into_iter()
                .map(|(id, policy, _)| (id, policy))
                .collect()
        })
        .unwrap_or_default()
}

pub fn trust_allows_script(policy: Option<&str>, blanket_scripts: bool) -> bool {
    match policy {
        Some("approved") => true,
        Some("denied") => false,
        _ => blanket_scripts,
    }
}
