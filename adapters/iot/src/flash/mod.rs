//! Flash firmware xuống thiết bị IoT — FAIL-CLOSED.
//! Chưa có exec passthrough (P2) thì báo lỗi rõ ràng, KHÔNG BAO GIỜ giả success.
// (Flash firmware to IoT devices — FAIL-CLOSED: until exec passthrough lands (P2),
// report a clear error instead of faking success.)

use crate::framework::IotFramework;
use mgc_exec::run::{run as mgc_run, ExecOptions};
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
        IotFramework::Esp32Rust => flash_esp32(_project_root, _port).await,
        IotFramework::Platformio => flash_platformio(_project_root, _port).await,
        IotFramework::Zephyr => flash_zephyr(_project_root, _port).await,
    }
}

#[derive(Debug, Clone)]
pub struct FlashResult {
    pub port: String,
    pub success: bool,
    pub duration_ms: u64,
}

/// Chạy tool passthrough qua mgc-exec (allowlist + audit log) — REAL exec.
fn exec_tool(tool: &str, args: &[&str], root: &Path) -> MgResult<()> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let opts = ExecOptions {
        cwd: Some(root.to_path_buf()),
        ..Default::default()
    };
    let report = mgc_run(tool, &args, &opts).map_err(|e| MgError::Other(format!("{tool}: {e}")))?;
    if report.exit_code == 0 {
        Ok(())
    } else {
        Err(MgError::Other(format!(
            "{tool} exited with {}: {}",
            report.exit_code,
            report.stderr_tail.trim()
        )))
    }
}

async fn flash_esp32(root: &Path, port: Option<&str>) -> MgResult<FlashResult> {
    let started = std::time::Instant::now();
    let mut args: Vec<String> = vec!["flash".into()];
    if let Some(p) = port {
        args.push("--port".into());
        args.push(p.into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    exec_tool("espflash", &refs, root)?;
    Ok(FlashResult {
        port: port.unwrap_or("auto").into(),
        success: true,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

async fn flash_platformio(root: &Path, port: Option<&str>) -> MgResult<FlashResult> {
    let started = std::time::Instant::now();
    let mut args: Vec<String> = vec!["run".into(), "--target".into(), "upload".into()];
    if let Some(p) = port {
        args.push("--upload-port".into());
        args.push(p.into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    exec_tool("pio", &refs, root)?;
    Ok(FlashResult {
        port: port.unwrap_or("auto").into(),
        success: true,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

async fn flash_zephyr(root: &Path, _port: Option<&str>) -> MgResult<FlashResult> {
    let started = std::time::Instant::now();
    exec_tool("west", &["flash"], root)?;
    Ok(FlashResult {
        port: "west-managed".into(),
        success: true,
        duration_ms: started.elapsed().as_millis() as u64,
    })
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
