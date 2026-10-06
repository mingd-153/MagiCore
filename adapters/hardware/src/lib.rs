#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-hardware-adapter — hardware ecosystem adapter for MagiCore.
//! Hardware core hỗ trợ optimizer/bench add-on qua module rõ trách nhiệm.

mod adapter;
mod detection;
mod sbom;

pub use adapter::{HardwareAdapter, adapter_for};
pub use sbom::generate_sbom;
