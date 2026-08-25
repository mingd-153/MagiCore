//! mgc-iot-adapter — IoT ecosystem adapter for MagiCore.
//! IoT core hỗ trợ ESP32 Rust, PlatformIO và Zephyr qua module rõ trách nhiệm.

mod adapter;
mod framework;
mod sbom;
mod tooling;

pub use adapter::{adapter_for, IotAdapter};
pub use framework::{board_target, detect_framework, known_boards, IotFramework, KNOWN_BOARDS};
pub use sbom::generate_sbom;
