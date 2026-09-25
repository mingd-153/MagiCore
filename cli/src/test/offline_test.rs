#![cfg(test)]
#![allow(clippy::unwrap_used)]
// Tests mutate env single-threaded (edition 2024 unsafe rule) — test đổi env 1 luồng.
#![allow(unsafe_code)]
//! Tests for offline mode env-based state — kiểm tra trạng thái offline qua env.
//! Single test body: env vars are process-global, so flag assertions must run
//! sequentially inside ONE test to avoid racing parallel tests.
//! Env var là process-global nên mọi assertion gom trong 1 test chạy tuần tự.

use super::*;

#[test]
fn test_offline_mode_flag_lifecycle() {
    // Default off — mặc định tắt (SAFETY: sequential single-test env writes).
    unsafe { std::env::remove_var("MGC_OFFLINE_MODE") };
    assert!(!is_offline_mode());

    // set_offline_mode(true/false) toggles the same env flag adapters read
    // — bật/tắt đúng env flag mà adapter đọc.
    set_offline_mode(true);
    assert!(is_offline_mode());
    set_offline_mode(false);
    assert!(!is_offline_mode());

    // Recognized "on" values — các giá trị đọc là bật
    // SAFETY: single test body, sequential env writes (no parallel test reads
    // this key — the other offline assertions live in this same test).
    // AN TOÀN: 1 thân test, ghi env tuần tự — không test song song đọc key này.
    unsafe {
        for value in ["1", "true", "yes", "on"] {
            std::env::set_var("MGC_OFFLINE_MODE", value);
            assert!(is_offline_mode(), "expected '{value}' to read as on");
        }

        // Recognized "off" values — các giá trị đọc là tắt
        for value in ["0", "false", "off", ""] {
            std::env::set_var("MGC_OFFLINE_MODE", value);
            assert!(!is_offline_mode(), "expected '{value}' to read as off");
        }
    }

    // Cleanup — dọn env sau test (SAFETY: sequential single-test env writes).
    unsafe { std::env::remove_var("MGC_OFFLINE_MODE") };
    assert!(!is_offline_mode());
}
