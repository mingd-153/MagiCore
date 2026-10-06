//! IoT framework and board detection for mgc-iot-adapter.
//! Tách nhận diện framework/board để adapter IoT dễ mở rộng.

use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

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

/// Legacy board-map snapshot kept for source compatibility.
/// Snapshot map board cũ được giữ để tương thích mã nguồn.
/// New code should use [`boards_for_framework`] or [`board_target_for_framework`].
/// Mã mới nên dùng [`boards_for_framework`] hoặc [`board_target_for_framework`].
pub const KNOWN_BOARDS: &[(&str, &str, &str)] = &[
    ("esp32", "esp32", "xtensa-esp32-none-elf"),
    ("esp32c3", "esp32c3", "riscv32imac-unknown-none-elf"),
    ("esp32s3", "esp32s3", "xtensa-esp32s3-none-elf"),
    ("esp32dev", "esp32", "xtensa-esp32-none-elf"),
    ("nodemcu-32s", "esp32", "xtensa-esp32-none-elf"),
    ("nrf52dk_nrf52832", "nrf52", "thumbv7em-none-eabihf"),
    ("stm32f4_disc", "stm32", "thumbv7em-none-eabihf"),
];

const BOARD_REGISTRY_JSON: &str = include_str!("../../../assets/boards/registry.json");

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BoardInfo {
    /// Stable board identifier used by project configuration.
    /// ID ổn định dùng trong cấu hình project.
    pub id: String,
    /// Human-readable choice shown by the IoT wizard.
    /// Nhãn dễ đọc hiển thị trong wizard IoT.
    pub label: String,
    /// Chip-family label retained from the existing board map.
    /// Nhãn họ chip giữ nguyên từ map board hiện tại.
    pub chip: String,
    /// Rust target triple retained from the existing board map.
    /// Rust target triple giữ nguyên từ map board hiện tại.
    pub target: String,
}

#[derive(Debug, Deserialize)]
struct BoardRegistry {
    schema_version: u32,
    boards: Vec<BoardRecord>,
}

#[derive(Debug, Deserialize)]
struct BoardRecord {
    id: String,
    label: String,
    chip: String,
    target: String,
    wizard_order: u32,
    frameworks: Vec<String>,
}

fn board_registry() -> &'static BoardRegistry {
    static REGISTRY: OnceLock<BoardRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let registry: BoardRegistry = serde_json::from_str(BOARD_REGISTRY_JSON)
            .expect("embedded IoT board registry must be valid JSON");
        assert_eq!(
            registry.schema_version, 1,
            "unsupported board registry schema"
        );
        assert!(
            !registry.boards.is_empty(),
            "embedded board registry is empty"
        );

        let mut ids = std::collections::HashSet::new();
        let mut choices = std::collections::HashSet::new();
        for board in &registry.boards {
            assert!(!board.id.trim().is_empty(), "board id must not be empty");
            assert!(
                !board.label.trim().is_empty(),
                "board label must not be empty"
            );
            assert!(
                !board.chip.trim().is_empty(),
                "board chip must not be empty"
            );
            assert!(
                !board.target.trim().is_empty(),
                "board target must not be empty"
            );
            assert!(
                ids.insert(board.id.as_str()),
                "duplicate board id in registry"
            );
            assert!(!board.frameworks.is_empty(), "board must name a framework");
            assert!(
                board.frameworks.iter().all(|framework| {
                    IotFramework::from_str(framework)
                        .is_some_and(|parsed| parsed.as_str() == framework)
                }),
                "board registry framework ids must be canonical"
            );
            assert!(
                board
                    .frameworks
                    .iter()
                    .all(|framework| { choices.insert((framework.as_str(), board.wizard_order)) }),
                "board wizard order must be unique within each framework"
            );
        }
        registry
    })
}

fn canonical_framework(framework: &str) -> Option<&'static str> {
    IotFramework::from_str(framework).map(|parsed| parsed.as_str())
}

/// Return board choices that the selected framework can actually scaffold.
/// Trả các board mà framework đã chọn thật sự có thể scaffold.
/// Unknown framework identifiers return an empty list.
/// Framework không biết sẽ trả danh sách rỗng.
pub fn boards_for_framework(framework: &str) -> Vec<BoardInfo> {
    let Some(framework) = canonical_framework(framework) else {
        return Vec::new();
    };
    let mut boards = board_registry()
        .boards
        .iter()
        .filter(|board| board.frameworks.iter().any(|item| item == framework))
        .collect::<Vec<_>>();
    boards.sort_by_key(|board| board.wizard_order);
    boards
        .into_iter()
        .map(|board| BoardInfo {
            id: board.id.clone(),
            label: board.label.clone(),
            chip: board.chip.clone(),
            target: board.target.clone(),
        })
        .collect()
}

pub fn known_boards() -> Vec<(String, String, String)> {
    board_registry()
        .boards
        .iter()
        .map(|board| (board.id.clone(), board.chip.clone(), board.target.clone()))
        .collect()
}

pub fn board_target(board: &str) -> Option<String> {
    board_registry()
        .boards
        .iter()
        .find(|entry| entry.id == board)
        .map(|entry| entry.target.clone())
}

/// Resolve a board target only when that board belongs to the selected framework.
/// Chỉ lấy target nếu board thuộc framework đã chọn.
pub fn board_target_for_framework(framework: &str, board: &str) -> Option<String> {
    boards_for_framework(framework)
        .into_iter()
        .find(|entry| entry.id == board)
        .map(|entry| entry.target)
}

pub fn detect_framework(root: &Path) -> Option<IotFramework> {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(Some(content)) =
        mgc_config::project::read_regular_project_text(&root.join("mgc.toml"), "project config")
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem")
            .and_then(|e| e.as_str())
            .is_some_and(|eco| eco != "iot")
        {
            return None;
        }
        if let Some(fw) = v
            .get("iot")
            .and_then(|i| i.get("framework"))
            .and_then(|f| f.as_str())
            && let Some(framework) = IotFramework::from_str(fw)
        {
            return Some(framework);
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
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(Some(content)) =
        mgc_config::project::read_regular_project_text(&root.join("mgc.toml"), "project config")
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem").and_then(|e| e.as_str()) == Some("iot") {
            return true;
        }
        if v.get("iot").is_some() {
            return true;
        }
    }
    root.join("platformio.ini").exists()
        || root.join("west.yml").exists()
        || root.join("Cargo.toml").exists()
}

pub(crate) fn target_from_manifest(root: &Path) -> Option<String> {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(Some(content)) =
        mgc_config::project::read_regular_project_text(&root.join("mgc.toml"), "project config")
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
        && let Some(target) = v
            .get("iot")
            .and_then(|i| i.get("target"))
            .and_then(|t| t.as_str())
    {
        return Some(target.to_string());
    }
    None
}
