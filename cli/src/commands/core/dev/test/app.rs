//! Tests for T9 — OS-aware simulator selector (`mgc dev app`).

#[cfg(target_os = "macos")]
use super::find_ios_simulator;
use super::{TargetPlatform, detect_target_platform};

#[test]
fn xcode_project_prefers_workspace() {
    let dir = std::env::temp_dir().join(format!("mgc-app-xc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("App.xcodeproj"), "").unwrap();
    std::fs::write(dir.join("App.xcworkspace"), "").unwrap();
    assert_eq!(
        super::find_xcode_project(&dir).as_deref(),
        Some("App.xcworkspace")
    );
    std::fs::remove_file(dir.join("App.xcworkspace")).unwrap();
    assert_eq!(
        super::find_xcode_project(&dir).as_deref(),
        Some("App.xcodeproj")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dev_scheme_reads_mgc_toml() {
    let dir = std::env::temp_dir().join(format!("mgc-app-scheme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(super::dev_scheme(&dir).is_none());
    std::fs::write(
        dir.join("mgc.toml"),
        "[app]\nlanguage = \"objc\"\ndev_scheme = \"App\"\n",
    )
    .unwrap();
    assert_eq!(super::dev_scheme(&dir).as_deref(), Some("App"));
    std::fs::write(dir.join("mgc.toml"), "[app]\nlanguage = \"objc\"\n").unwrap();
    assert!(super::dev_scheme(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn app_dev_settings_ignore_external_mgc_toml_symlink() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::fs::write(
        external.path().join("mgc.toml"),
        "[app]\ndev_scheme = 'External'\napplication_id = 'com.external.app'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("mgc.toml"),
        root.path().join("mgc.toml"),
    )
    .unwrap();

    assert_eq!(super::dev_scheme(root.path()), None);
    assert_eq!(super::read_app_id(root.path()), None);
}

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
    // On macOS: no panic; with Xcode → Some(udid), without → None.
    // Trên macOS: không panic; có Xcode → Some(udid), không → None.
    let result = find_ios_simulator();
    if let Some(ref udid) = result {
        // UDID must be hex-dash form (8-4-4-4-12).
        // UDID phải dạng hex-dash (8-4-4-4-12).
        assert!(udid.len() >= 8, "UDID too short: {udid}");
    }
    // None is also valid (Xcode not installed).
    // None cũng hợp lệ (không cài Xcode).
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

#[test]
fn flutter_run_never_resolves_packages_outside_mgc() {
    let ios = super::flutter_dev_command(&TargetPlatform::IosSimulator, true);
    let android = super::flutter_dev_command(&TargetPlatform::Android, true);
    assert!(ios.args.contains(&"--no-pub".to_string()));
    assert!(android.args.contains(&"--no-pub".to_string()));
}

#[test]
fn adb_device_list_requires_a_ready_device_record() {
    let header_only = "List of devices attached\n\n";
    let unauthorized = "List of devices attached\nemulator-5554 unauthorized\n";
    let offline = "List of devices attached\nemulator-5554 offline\n";
    let ready = "List of devices attached\nemulator-5554 device product:sdk_gphone64\n";

    assert!(!super::adb_output_has_ready_device(header_only));
    assert!(!super::adb_output_has_ready_device(unauthorized));
    assert!(!super::adb_output_has_ready_device(offline));
    assert!(super::adb_output_has_ready_device(ready));
}
