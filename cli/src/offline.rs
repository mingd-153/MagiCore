//! Offline mode state — propagated via `InstallOptions.offline` and the
//! `MGC_OFFLINE_MODE` env var (read by adapters: resolve/install gates).
//! Trạng thái offline — truyền qua InstallOptions.offline và env MGC_OFFLINE_MODE
//! (adapter đọc để đóng cổng resolve/install fail-closed).

/// Enable offline mode globally (env var) — bật offline toàn cục qua env var.
/// Adapters check this flag before any network-dependent resolution path.
/// SAFETY: mgc sets the flag at install entry (before adapters spawn worker
/// threads); CLI commands run one at a time — single writer, no race.
/// AN TOÀN: ghi ở install entry trước khi adapter spawn thread; 1 writer.
#[allow(unsafe_code)]
pub fn set_offline_mode(offline: bool) {
    unsafe {
        if offline {
            std::env::set_var("MGC_OFFLINE_MODE", "1");
        } else {
            std::env::remove_var("MGC_OFFLINE_MODE");
        }
    }
}

/// Check if offline mode is enabled — kiểm tra offline mode có bật không.
pub fn is_offline_mode() -> bool {
    std::env::var("MGC_OFFLINE_MODE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

#[cfg(test)]
#[path = "test/offline_test.rs"]
mod tests;
