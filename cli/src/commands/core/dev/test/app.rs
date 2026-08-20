//! Tests for T9 — OS-aware simulator selector (`mg dev app`).

use super::{detect_target_platform, find_ios_simulator, TargetPlatform};

#[test]
fn detect_target_platform_returns_valid_variant() {
    // Không crash — luôn trả về một trong 2 variant hợp lệ
    let platform = detect_target_platform();
    assert!(
        platform == TargetPlatform::IosSimulator || platform == TargetPlatform::Android,
        "expected IosSimulator or Android, got {:?}",
        platform
    );
}

#[cfg(target_os = "macos")]
#[test]
fn find_ios_simulator_returns_some_or_none_without_panic() {
    // Trên macOS: không panic; nếu Xcode có → Some(udid), không → None
    let result = find_ios_simulator();
    if let Some(ref udid) = result {
        // UDID phải dạng hex-dash (8-4-4-4-12)
        assert!(udid.len() >= 8, "UDID quá ngắn: {udid}");
    }
    // None cũng hợp lệ (Xcode không cài)
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_always_targets_android() {
    // Trên Linux/Windows: platform phải là Android (không bao giờ iOS)
    let platform = detect_target_platform();
    assert_eq!(
        platform,
        TargetPlatform::Android,
        "non-macOS must target Android"
    );
}

#[test]
fn target_platform_debug_format() {
    // Smoke test: Debug trait hoạt động
    let _ = format!("{:?}", TargetPlatform::IosSimulator);
    let _ = format!("{:?}", TargetPlatform::Android);
}
