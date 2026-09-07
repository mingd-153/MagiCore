#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-game-adapter — game ecosystem adapter for MagiCore.
//! Game core hỗ trợ Bevy/Godot/Unity/Unreal qua module rõ trách nhiệm.

mod adapter;
mod engine;
mod sbom;
mod tooling;

pub mod cache;
pub mod dev;
pub mod install;
pub mod scaffold;

pub use adapter::{GameAdapter, adapter_for};
pub use engine::{GameEngine, detect_engine};
pub use sbom::generate_sbom;
