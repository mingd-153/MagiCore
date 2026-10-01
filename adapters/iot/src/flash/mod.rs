//! Firmware flashing contract for IoT projects; no native backend is implemented yet.
//! Hợp đồng flash firmware IoT; hiện chưa có backend native.

use crate::framework::IotFramework;
use mgc_types::MgResult;
use std::path::Path;

/// Flash firmware only when a MagiCore-owned backend exists.
/// Chỉ flash firmware khi có backend do MagiCore sở hữu.
pub async fn flash_firmware(
    framework: IotFramework,
    _project_root: &Path,
    _port: Option<&str>,
) -> MgResult<FlashResult> {
    // Do not disguise an external flasher process as MagiCore-owned behavior.
    // Không biến process flasher bên ngoài thành chức năng do MagiCore sở hữu.
    Err(mgc_types::capabilities::unsupported_capability(
        "iot",
        "flash",
        &format!(
            "{} has no MagiCore-owned firmware flashing backend; external flash tools are not invoked",
            framework.as_str()
        ),
    ))
}

#[derive(Debug, Clone)]
pub struct FlashResult {
    pub port: String,
    pub success: bool,
    pub duration_ms: u64,
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
/// Unix: scan /dev for ttyUSB*/ttyACM*/tty.usb*/cu.usb*
/// Windows: use serialport crate SetupAPI when windows-serial feature enabled
// (Detect available USB serial ports. Windows needs SetupAPI enumeration via the
// `serialport` crate (feature windows-serial) — returns empty when disabled.)
pub fn detect_serial_ports() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        #[cfg(feature = "windows-serial")]
        {
            match serialport::available_ports() {
                Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
                Err(_) => Vec::new(),
            }
        }
        #[cfg(not(feature = "windows-serial"))]
        {
            Vec::new()
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        detect_serial_ports_in(Path::new("/dev"))
    }
}

#[cfg(test)]
#[path = "test/flash_tests.rs"]
mod tests;
