//! Flash firmware xuống thiết bị IoT — FAIL-CLOSED.
//! Chưa có exec passthrough (P2) thì báo lỗi rõ ràng, KHÔNG BAO GIỜ giả success.
// (Flash firmware to IoT devices — FAIL-CLOSED: until exec passthrough lands (P2),
// report a clear error instead of faking success.)

use crate::framework::IotFramework;
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Flash firmware to device — tham số giữ nguyên chữ ký cho P2 nối mgc-exec.
// (Flash firmware to device — params kept so P2 can wire mgc-exec without API churn.)
pub async fn flash_firmware(
    framework: IotFramework,
    _project_root: &Path,
    _port: Option<&str>,
) -> MgResult<FlashResult> {
    match framework {
        IotFramework::Esp32Rust => flash_esp32().await,
        IotFramework::Platformio => flash_platformio().await,
        IotFramework::Zephyr => flash_zephyr().await,
    }
}

#[derive(Debug, Clone)]
pub struct FlashResult {
    pub port: String,
    pub success: bool,
    pub duration_ms: u64,
}

async fn flash_esp32() -> MgResult<FlashResult> {
    // Fail-closed (RULE §11): chạy espflash thật qua mgc-exec ở P2 — giờ không được trả success ảo
    // (Fail-closed: real espflash runs via mgc-exec in P2 — never return a fake success today)
    Err(MgError::Other(
        "esp32-rust flash not implemented yet (P2): run `espflash flash <elf>` manually meanwhile"
            .into(),
    ))
}

async fn flash_platformio() -> MgResult<FlashResult> {
    Err(MgError::Other(
        "platformio upload not implemented yet (P2): run `pio run --target upload` manually meanwhile".into(),
    ))
}

async fn flash_zephyr() -> MgResult<FlashResult> {
    Err(MgError::Other(
        "zephyr flash not implemented yet (P2): run `west flash` manually meanwhile".into(),
    ))
}

/// Quét cổng serial trong 1 thư mục (hàm thuần — test được với TempDir).
/// Match prefix chuẩn Linux/macOS: ttyUSB*, ttyACM*, tty.usb*, cu.usb*.
// (Scan a directory for serial device names — pure fn, testable with TempDir.
// Matches standard Linux/macOS prefixes: ttyUSB*, ttyACM*, tty.usb*, cu.usb*.)
pub fn detect_serial_ports_in(base: &Path) -> Vec<String> {
    const PREFIXES: &[&str] = &["ttyUSB", "ttyACM", "tty.usb", "cu.usb"];
    let mut ports = Vec::new();
    let Ok(entries) = std::fs::read_dir(base) else {
        return ports;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if PREFIXES.iter().any(|p| name.starts_with(p)) {
            ports.push(base.join(&file_name).to_string_lossy().into_owned());
        }
    }
    ports.sort();
    ports
}

/// Dò cổng serial USB khả dụng trên máy hiện tại.
/// Windows cần enumerate COM qua SetupAPI (crate `serialport`, P2) — tạm trả rỗng.
// (Detect available USB serial ports. Windows needs SetupAPI enumeration via the
// `serialport` crate (P2) — returns empty there for now.)
pub fn detect_serial_ports() -> Vec<String> {
    if cfg!(target_os = "windows") {
        Vec::new()
    } else {
        detect_serial_ports_in(Path::new("/dev"))
    }
}

#[cfg(test)]
#[path = "test/flash_tests.rs"]
mod tests;
