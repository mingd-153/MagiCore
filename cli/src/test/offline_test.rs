#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for offline mode thread-local state

use super::*;

#[test]
fn test_offline_mode_default_false() {
    reset_offline_mode();
    assert!(!is_offline_mode());
}

#[test]
fn test_set_offline_mode() {
    reset_offline_mode();
    set_offline_mode(true);
    assert!(is_offline_mode());
    set_offline_mode(false);
    assert!(!is_offline_mode());
}

#[test]
fn test_thread_local_isolation() {
    reset_offline_mode();
    set_offline_mode(true);
    std::thread::spawn(|| {
        // Different thread → default false
        assert!(!is_offline_mode());
    })
    .join()
    .unwrap();
    // Main thread still true
    assert!(is_offline_mode());
}
