//! Integration tests for mg-iot-adapter — sát với src/lib.rs
//! Kiểm thử: detect_framework (ESP32-Rust, PlatformIO, Zephyr), board mapping, PackageAdapter trait.

use mg_iot_adapter::{adapter_for, detect_framework, IotFramework};
use mg_types::adapter::{AddOptions, PackageAdapter};
use mg_types::PackageName;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mg-iot-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── detect_framework ───────────────────────────────────────────────────────

#[test]
fn detect_platformio_via_ini() {
    let dir = tmp("pio");
    std::fs::write(
        dir.join("platformio.ini"),
        "[env:esp32dev]\nplatform = espressif32\n",
    )
    .unwrap();
    assert_eq!(detect_framework(&dir), Some(IotFramework::Platformio));
}

#[test]
fn detect_zephyr_via_west_yml() {
    let dir = tmp("zephyr");
    std::fs::write(dir.join("west.yml"), "manifest:\n  projects: []\n").unwrap();
    assert_eq!(detect_framework(&dir), Some(IotFramework::Zephyr));
}

#[test]
fn detect_esp32_rust_via_mg_toml() {
    let dir = tmp("esp32-rust");
    std::fs::write(
        dir.join("mg.toml"),
        "ecosystem = \"iot\"\n\n[iot]\nframework = \"esp32-rust\"\n",
    )
    .unwrap();
    assert_eq!(detect_framework(&dir), Some(IotFramework::Esp32Rust));
}

#[test]
fn detect_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(detect_framework(&dir).is_none());
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_for_platformio() {
    let dir = tmp("af-pio");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.framework(), "platformio");
}

#[test]
fn adapter_for_returns_none_for_plain_dir() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).is_none());
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let dir = tmp("name-eco");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.name(), "iot");
    assert_eq!(format!("{:?}", a.ecosystem()), "Iot");
}

#[test]
fn can_handle_returns_true_for_iot_project() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert!(a.can_handle(&dir));
}

#[tokio::test]
async fn platformio_add_delegates_to_pio_tool() {
    let dir = tmp("add-pio");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("arduino-json").unwrap();
    let result = a.add(&dir, &name, None, AddOptions::default()).await;
    // pio pkg install được gọi — trong test env không có binary pio -> expect error về spawn pio
    match &result {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("pio") || msg.contains("No such") || msg.contains("not found") || msg.contains("os error"),
                "unexpected error: {msg}"
            );
        }
    }
}

#[tokio::test]
async fn zephyr_add_fails_closed_directing_to_west_yml() {
    let dir = tmp("add-zephyr");
    std::fs::write(dir.join("west.yml"), "manifest:\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("lvgl").unwrap();
    let err = a.add(&dir, &name, None, AddOptions::default()).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("zephyr") || msg.contains("west.yml") || msg.contains("prj.conf"),
        "error must mention west.yml/prj.conf: {msg}"
    );
}

#[tokio::test]
async fn audit_returns_clean_for_iot_project() {
    let dir = tmp("audit-iot");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}
