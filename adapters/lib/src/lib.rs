//! mgc-lib-adapter — library ecosystem adapter for MagiCore.
//! Lib core hỗ trợ TypeScript/Rust/Python qua module rõ trách nhiệm.

mod adapter;
mod language;
mod manifest;
mod sbom;
mod tooling;

pub use adapter::{adapter_for, adapter_for_with_chain, LibAdapter};
pub use sbom::generate_sbom;
pub use tooling::check_pip_allowed;
