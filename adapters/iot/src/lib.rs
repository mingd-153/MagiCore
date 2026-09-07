#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-iot-adapter — IoT ecosystem adapter for MagiCore.
//! IoT core hỗ trợ ESP32 Rust, PlatformIO và Zephyr qua module rõ trách nhiệm.

mod adapter;
mod framework;
mod sbom;
mod tooling;

pub mod flash;
pub mod install;
pub mod scaffold;

pub use adapter::{IotAdapter, adapter_for};
pub use framework::{IotFramework, KNOWN_BOARDS, board_target, detect_framework, known_boards};
pub use sbom::generate_sbom;
