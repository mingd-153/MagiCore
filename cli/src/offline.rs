//! Offline mode state management
//! Quản lý trạng thái offline mode

use std::cell::Cell;

thread_local! {
    /// Thread-local offline mode flag — Cờ offline mode thread-local
    static OFFLINE_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Set offline mode for current thread — Đặt offline mode cho thread hiện tại
pub fn set_offline_mode(offline: bool) {
    OFFLINE_MODE.with(|f| f.set(offline));
}

/// Check if offline mode is enabled — Kiểm tra offline mode có bật không
pub fn is_offline_mode() -> bool {
    OFFLINE_MODE.with(|f| f.get())
}

/// Reset offline mode (for tests) — Reset offline mode (cho tests)
#[cfg(test)]
pub fn reset_offline_mode() {
    OFFLINE_MODE.with(|f| f.set(false));
}

#[cfg(test)]
#[path = "test/offline_test.rs"]
mod tests;
