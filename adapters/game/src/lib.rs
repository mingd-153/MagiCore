//! mgc-game-adapter — game ecosystem adapter for MagiCore.
//! Game core hỗ trợ Bevy/Godot/Unity/Unreal qua module rõ trách nhiệm.

mod adapter;
mod engine;
mod sbom;
mod tooling;

pub use adapter::{adapter_for, GameAdapter};
pub use engine::{detect_engine, GameEngine};
pub use sbom::generate_sbom;
