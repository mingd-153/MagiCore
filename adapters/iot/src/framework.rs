//! IoT framework and board detection for mgc-iot-adapter.
//! Tách nhận diện framework/board để adapter IoT dễ mở rộng.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IotFramework {
    Esp32Rust,
    Platformio,
    Zephyr,
}

impl IotFramework {
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s {
            "esp32-rust" => Some(Self::Esp32Rust),
            "platformio" => Some(Self::Platformio),
            "zephyr" | "zephyr-arm" => Some(Self::Zephyr),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Esp32Rust => "esp32-rust",
            Self::Platformio => "platformio",
            Self::Zephyr => "zephyr",
        }
    }
}

pub const KNOWN_BOARDS: &[(&str, &str, &str)] = &[
    ("esp32", "esp32", "xtensa-esp32-none-elf"),
    ("esp32c3", "esp32c3", "riscv32imac-unknown-none-elf"),
    ("esp32s3", "esp32s3", "xtensa-esp32s3-none-elf"),
    ("esp32dev", "esp32dev", "riscv32imac-unknown-none-elf"),
    ("nodemcu-32s", "esp32", "xtensa-esp32-none-elf"),
    ("nrf52dk_nrf52832", "nrf52", "thumbv7em-none-eabihf"),
    ("stm32f4_disc", "stm32", "thumbv7em-none-eabihf"),
];

pub fn known_boards() -> Vec<(String, String, String)> {
    KNOWN_BOARDS
        .iter()
        .map(|(id, chip, target)| (id.to_string(), chip.to_string(), target.to_string()))
        .collect()
}

pub fn board_target(board: &str) -> Option<String> {
    KNOWN_BOARDS
        .iter()
        .find(|(id, _, _)| *id == board)
        .map(|(_, _, target)| target.to_string())
}

pub fn detect_framework(root: &Path) -> Option<IotFramework> {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco != "iot" {
                    return None;
                }
            }
            if let Some(fw) = v
                .get("iot")
                .and_then(|i| i.get("framework"))
                .and_then(|f| f.as_str())
            {
                if let Some(framework) = IotFramework::from_str(fw) {
                    return Some(framework);
                }
            }
        }
    }
    if root.join("platformio.ini").exists() {
        return Some(IotFramework::Platformio);
    }
    if root.join("west.yml").exists() {
        return Some(IotFramework::Zephyr);
    }
    if root.join("Cargo.toml").exists() {
        return Some(IotFramework::Esp32Rust);
    }
    None
}

pub(crate) fn manifest_is_iot(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "iot" {
                    return true;
                }
            }
            if v.get("iot").is_some() {
                return true;
            }
        }
    }
    root.join("platformio.ini").exists()
        || root.join("west.yml").exists()
        || root.join("Cargo.toml").exists()
}

pub(crate) fn target_from_manifest(root: &Path) -> Option<String> {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(target) = v
                .get("iot")
                .and_then(|i| i.get("target"))
                .and_then(|t| t.as_str())
            {
                return Some(target.to_string());
            }
        }
    }
    None
}
