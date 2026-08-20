//! `mg dev app` — T9: OS-aware simulator selector.
//!
//! ## Quy tắc chọn nền tảng (T9 spec — 2026-08-19)
//!
//! | OS host | Nền tảng ưu tiên | Tool | Lý do |
//! |---------|-----------------|------|-------|
//! | macOS   | iOS Simulator   | `xcrun simctl` (chọn thiết bị available) | Xcode có sẵn trên mac |
//! | macOS   | Android fallback (nếu thiếu Xcode) | `flutter run -d emulator` | |
//! | Linux / Windows | Android Emulator | `adb` / `flutter run -d emulator` | iOS không chạy được |
//!
//! Với Flutter (đa nền tảng): detect OS → thêm đúng `-d` device flag.
//! Với Swift / ObjC (iOS-only): chỉ chạy trên macOS, báo lỗi rõ trên Linux/Win.
//! Với Kotlin (Android-only): `./gradlew installDebug` + `adb shell am start`.

use anyhow::{bail, Result};
use std::path::Path;

use crate::commands::core::install::app::{
    find_xcode_project, language, project_root, run_tool, InstallCommand,
};

// ─── OS detection ────────────────────────────────────────────────────────────

/// Nền tảng đích khi chạy `mg dev app`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetPlatform {
    /// iOS Simulator (chỉ macOS + Xcode)
    IosSimulator,
    /// Android Emulator / Device
    Android,
}

/// Chọn nền tảng đích dựa theo OS host và tool có sẵn.
/// macOS + Xcode → iOS Simulator; còn lại → Android.
pub fn detect_target_platform() -> TargetPlatform {
    if cfg!(target_os = "macos") && xcode_available() {
        TargetPlatform::IosSimulator
    } else {
        TargetPlatform::Android
    }
}

/// Kiểm tra Xcode CLI tools có sẵn (xcrun tồn tại và simctl hoạt động).
fn xcode_available() -> bool {
    std::process::Command::new("xcrun")
        .args(["simctl", "list", "devices", "--json"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ─── iOS Simulator detection ──────────────────────────────────────────────────

/// Lấy UDID của simulator iOS đang booted (ưu tiên) hoặc simulator available đầu tiên.
/// Trả về `None` nếu không có simulator nào.
pub fn find_ios_simulator() -> Option<String> {
    let out = std::process::Command::new("xcrun")
        .args(["simctl", "list", "devices", "--json"])
        .output()
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let devices = json.get("devices")?.as_object()?;

    let mut booted_udid: Option<String> = None;
    let mut any_available_udid: Option<String> = None;

    // Duyệt tất cả runtime (iOS 17/18...) tìm device sẵn
    for (_runtime, devs) in devices {
        let Some(arr) = devs.as_array() else { continue };
        for dev in arr {
            let name = dev.get("name").and_then(|v| v.as_str()).unwrap_or("");
            // Chỉ xét iPhone, bỏ Watch/TV/Vision
            if !name.to_lowercase().contains("iphone") {
                continue;
            }
            let avail = dev
                .get("isAvailable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !avail {
                continue;
            }
            let udid = dev.get("udid").and_then(|v| v.as_str())?;
            let state = dev.get("state").and_then(|v| v.as_str()).unwrap_or("");
            if state == "Booted" {
                booted_udid = Some(udid.to_string());
                break;
            }
            if any_available_udid.is_none() {
                any_available_udid = Some(udid.to_string());
            }
        }
        if booted_udid.is_some() {
            break;
        }
    }
    booted_udid.or(any_available_udid)
}

/// Boot simulator nếu chưa chạy (non-blocking — trả về khi boot xong).
fn boot_simulator(udid: &str) -> Result<()> {
    let status = std::process::Command::new("xcrun")
        .args(["simctl", "boot", udid])
        .status()?;
    // boot trả về lỗi nếu đã booted — bỏ qua
    let _ = status;
    Ok(())
}

// ─── Android detection ────────────────────────────────────────────────────────

/// Kiểm tra có AVD nào đang chạy qua adb không.
fn android_emulator_running() -> bool {
    std::process::Command::new("adb")
        .args(["devices"])
        .output()
        .map(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines()
                .any(|l| l.contains("emulator") || l.contains("device"))
        })
        .unwrap_or(false)
}

// ─── Dev command builders ─────────────────────────────────────────────────────

/// Sinh InstallCommand cho Flutter theo OS/platform.
///
/// macOS + Xcode → `flutter run -d <simulator_udid>`
/// Android       → `flutter run -d emulator` (hoặc attached device)
fn flutter_dev_command(platform: &TargetPlatform, dry_run: bool) -> InstallCommand {
    match platform {
        TargetPlatform::IosSimulator => {
            // Tìm simulator có sẵn
            let device_arg = find_ios_simulator().unwrap_or_else(|| {
                if !dry_run {
                    mg_ui::warning("No iOS simulator found — falling back to auto-detect device");
                }
                "auto".to_string()
            });
            if !dry_run {
                mg_ui::info(&format!("Target: iOS Simulator ({})", device_arg));
            }
            InstallCommand {
                tool: "flutter".to_string(),
                args: if device_arg == "auto" {
                    vec!["run".to_string()]
                } else {
                    vec!["run".to_string(), "-d".to_string(), device_arg]
                },
            }
        }
        TargetPlatform::Android => {
            if !dry_run {
                mg_ui::info("Target: Android (emulator / attached device)");
            }
            InstallCommand {
                tool: "flutter".to_string(),
                args: vec!["run".to_string(), "-d".to_string(), "android".to_string()],
            }
        }
    }
}

/// Sinh InstallCommand cho Kotlin (Android-only).
///
/// Dùng `./gradlew installDebug` → `adb shell am start` nếu build OK.
/// Trên macOS vẫn chạy Android bình thường qua emulator.
fn kotlin_dev_command(root: &Path) -> InstallCommand {
    // Kiểm tra có gradlew không (Gradle wrapper — chuẩn Android)
    let has_gradlew = root.join("gradlew").exists() || root.join("android/gradlew").exists();
    let gradle_bin = if has_gradlew { "./gradlew" } else { "gradle" };
    InstallCommand {
        tool: gradle_bin.to_string(),
        args: vec!["installDebug".to_string()],
    }
}

// ─── ObjC / Xcodebuild ────────────────────────────────────────────────────────

/// Lấy scheme từ mg.toml [app] dev_scheme.
fn dev_scheme(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("mg.toml")).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("app")
        .and_then(|a| a.get("dev_scheme"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// iOS/ObjC dev — phải chạy trên macOS + Xcode.
/// Tự động tìm simulator booted/available, boot nếu cần.
async fn dev_ios(root: &Path, lang_is_swift: bool, dry_run: bool) -> Result<()> {
    // T9: từ chối rõ ràng trên Linux/Windows
    if !cfg!(target_os = "macos") {
        bail!(
            "iOS Simulator only runs on macOS (host: {}). \
             Use `mg dev app` on macOS or switch to Android target.",
            std::env::consts::OS
        );
    }
    if !xcode_available() {
        bail!(
            "Xcode command-line tools not found. Install with: xcode-select --install\n\
             Or set [app] dev_scheme in mg.toml to use xcodebuild."
        );
    }

    // ObjC: dùng scheme từ mg.toml
    if !lang_is_swift {
        let Some(scheme) = dev_scheme(root) else {
            let Some(proj) = find_xcode_project(root) else {
                return Err(crate::error::xcode_project_missing_short());
            };
            return Err(crate::error::objc_dev_needs_xcode(&proj));
        };
        let simulator_udid = find_ios_simulator().unwrap_or_else(|| "iPhone 16".to_string());
        if !dry_run {
            boot_simulator(&simulator_udid).ok();
        }
        let args = vec![
            "-scheme".to_string(),
            scheme,
            "-destination".to_string(),
            format!("id={simulator_udid}"),
            "build".to_string(),
        ];
        if dry_run {
            mg_ui::info(&format!(
                "[dry-run] would run: xcodebuild {}",
                args.join(" ")
            ));
            return Ok(());
        }
        mg_ui::info(&format!("App dev (ObjC): xcodebuild {}", args.join(" ")));
        return run_tool(root, "xcodebuild", &args);
    }

    // Swift: `swift run` (macOS CLI) hoặc xcodebuild nếu có .xcodeproj
    let has_xcodeproj = find_xcode_project(root).is_some();
    if has_xcodeproj {
        let scheme = dev_scheme(root).unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "App".to_string())
        });
        let simulator_udid = find_ios_simulator().unwrap_or_else(|| "iPhone 16".to_string());
        if !dry_run {
            boot_simulator(&simulator_udid).ok();
        }
        let args = vec![
            "-scheme".to_string(),
            scheme,
            "-destination".to_string(),
            format!("id={simulator_udid}"),
            "run".to_string(),
        ];
        if dry_run {
            mg_ui::info(&format!(
                "[dry-run] would run: xcodebuild {}",
                args.join(" ")
            ));
            return Ok(());
        }
        mg_ui::info(&format!(
            "App dev (Swift/Xcode): xcodebuild {}",
            args.join(" ")
        ));
        return run_tool(root, "xcodebuild", &args);
    }

    // Fallback: swift run (Package.swift)
    if dry_run {
        mg_ui::info("[dry-run] would run: swift run");
        return Ok(());
    }
    mg_ui::info("App dev (Swift): swift run");
    run_tool(root, "swift", &["run".to_string()])
}

// ─── Main entry point ─────────────────────────────────────────────────────────

/// `mg dev app` — T9: chọn platform dựa theo OS host.
///
/// macOS + Xcode → iOS Simulator (tự tìm UDID booted/available)
/// Linux / Windows → Android Emulator (flutter -d android / gradlew installDebug)
pub async fn dev(dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;

    let platform = detect_target_platform();

    if dry_run {
        mg_ui::info(&format!(
            "[dry-run] mg dev app — OS: {}, platform: {:?}, lang: {:?}",
            std::env::consts::OS,
            platform,
            lang
        ));
    } else {
        mg_ui::info(&format!(
            "mg dev app — OS: {}, target: {}",
            std::env::consts::OS,
            match &platform {
                TargetPlatform::IosSimulator => "iOS Simulator",
                TargetPlatform::Android => "Android",
            }
        ));
    }

    match lang {
        mg_app_adapter::AppLanguage::Flutter => {
            let cmd = flutter_dev_command(&platform, dry_run);
            if dry_run {
                mg_ui::info(&format!(
                    "[dry-run] would run: {} {}",
                    cmd.tool,
                    cmd.args.join(" ")
                ));
                return Ok(());
            }
            mg_ui::info(&format!("Running: {} {}", cmd.tool, cmd.args.join(" ")));
            run_tool(&root, &cmd.tool, cmd.args.as_slice())?;
            Ok(())
        }

        mg_app_adapter::AppLanguage::Swift => dev_ios(&root, true, dry_run).await,

        mg_app_adapter::AppLanguage::ObjC => dev_ios(&root, false, dry_run).await,

        mg_app_adapter::AppLanguage::Kotlin => {
            // Kotlin = Android-only (không phân biệt OS host)
            if dry_run {
                mg_ui::info("[dry-run] would run: ./gradlew installDebug");
                return Ok(());
            }
            let cmd = kotlin_dev_command(&root);
            mg_ui::info(&format!(
                "App dev (Kotlin/Android): {} {}",
                cmd.tool,
                cmd.args.join(" ")
            ));
            run_tool(&root, &cmd.tool, cmd.args.as_slice())?;
            // Sau installDebug: launch app qua adb nếu có emulator
            if android_emulator_running() {
                // Đọc applicationId từ mg.toml nếu có
                let app_id = read_app_id(&root).unwrap_or_else(|| "com.example.app".to_string());
                mg_ui::info(&format!(
                    "Launching app: adb shell am start -n {app_id}/.MainActivity"
                ));
                let _ = std::process::Command::new("adb")
                    .args([
                        "shell",
                        "am",
                        "start",
                        "-n",
                        &format!("{app_id}/.MainActivity"),
                    ])
                    .status();
            }
            Ok(())
        }

        mg_app_adapter::AppLanguage::ReactNative => {
            // React Native: theo platform
            let rn_cmd = match &platform {
                TargetPlatform::IosSimulator => {
                    let udid = find_ios_simulator();
                    if !dry_run {
                        if let Some(ref u) = udid {
                            boot_simulator(u).ok();
                        }
                    }
                    InstallCommand {
                        tool: "npm".to_string(),
                        args: vec!["run".to_string(), "ios".to_string()],
                    }
                }
                TargetPlatform::Android => InstallCommand {
                    tool: "npm".to_string(),
                    args: vec!["run".to_string(), "android".to_string()],
                },
            };
            if dry_run {
                mg_ui::info(&format!(
                    "[dry-run] would run: {} {}",
                    rn_cmd.tool,
                    rn_cmd.args.join(" ")
                ));
                return Ok(());
            }
            mg_ui::info(&format!(
                "App dev (React Native/{}): {} {}",
                match platform {
                    TargetPlatform::IosSimulator => "iOS",
                    TargetPlatform::Android => "Android",
                },
                rn_cmd.tool,
                rn_cmd.args.join(" ")
            ));
            run_tool(&root, &rn_cmd.tool, rn_cmd.args.as_slice())?;
            Ok(())
        }

        mg_app_adapter::AppLanguage::Multi => {
            // Multi: ưu tiên Flutter entry
            let flutter_dir = root.join("flutter");
            if !flutter_dir.exists() {
                return Err(crate::error::multi_dev_flutter_only());
            }
            let cmd = flutter_dev_command(&platform, dry_run);
            if dry_run {
                mg_ui::info(&format!(
                    "[dry-run] would run: {} {} (in flutter/)",
                    cmd.tool,
                    cmd.args.join(" ")
                ));
                return Ok(());
            }
            mg_ui::info(&format!(
                "App dev (Multi → flutter/): {} {}",
                cmd.tool,
                cmd.args.join(" ")
            ));
            run_tool(&flutter_dir, &cmd.tool, cmd.args.as_slice())?;
            Ok(())
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Đọc applicationId từ mg.toml [app] application_id.
fn read_app_id(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("mg.toml")).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("app")
        .and_then(|a| a.get("application_id"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
