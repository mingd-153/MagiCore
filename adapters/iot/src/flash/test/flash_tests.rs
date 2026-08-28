#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Tests cho iot/flash — tách khỏi src theo RULE §5. Toàn bộ hermetic, không đụng phần cứng.
// (Tests for iot/flash — split per RULE §5. Fully hermetic, no hardware touched.)

use super::*;
use crate::framework::IotFramework;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_flash_esp32_fails_closed_not_fake_success() {
    let tmp = tmp();
    let err = flash_firmware(IotFramework::Esp32Rust, tmp.path(), None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("espflash"),
        "lỗi phải chỉ rõ tool passthrough: {err}"
    );
}

#[tokio::test]
async fn test_flash_all_frameworks_fail_closed() {
    let tmp = tmp();
    for fw in [
        IotFramework::Esp32Rust,
        IotFramework::Platformio,
        IotFramework::Zephyr,
    ] {
        assert!(
            flash_firmware(fw, tmp.path(), None).await.is_err(),
            "{fw:?} phải Err khi chưa có passthrough"
        );
    }
}

#[test]
fn test_detect_serial_ports_in_finds_standard_prefixes_sorted() {
    let tmp = tmp();
    for name in ["ttyACM1", "ttyUSB0", "ttyUSB10", "random.txt", "sdA"] {
        std::fs::write(tmp.path().join(name), b"").unwrap();
    }

    let ports = detect_serial_ports_in(tmp.path());
    assert_eq!(
        ports,
        vec![
            tmp.path().join("ttyACM1").to_string_lossy().into_owned(),
            tmp.path().join("ttyUSB0").to_string_lossy().into_owned(),
            tmp.path().join("ttyUSB10").to_string_lossy().into_owned(),
        ],
        "chỉ nhận prefix serial, sắp xếp thứ tự từ điển"
    );
}

#[test]
fn test_detect_serial_ports_in_macos_patterns() {
    let tmp = tmp();
    for name in ["tty.usbmodem123", "cu.usbserial-456"] {
        std::fs::write(tmp.path().join(name), b"").unwrap();
    }
    let ports = detect_serial_ports_in(tmp.path());
    assert_eq!(ports.len(), 2);
}

#[test]
fn test_detect_serial_ports_in_missing_dir_returns_empty() {
    let missing = std::path::PathBuf::from("/nonexistent/dev/dir");
    assert!(
        detect_serial_ports_in(&missing).is_empty(),
        "thư mục không tồn tại → rỗng, không panic"
    );
}
