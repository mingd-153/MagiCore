#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-app-adapter — mobile app ecosystem adapter for MagiCore.
//! App core hỗ trợ Flutter/Kotlin/Swift/React Native/ObjC/Multi qua module rõ trách nhiệm.

mod adapter;
mod language;
mod sbom;

pub mod audit;
pub mod cache;
pub mod install;
pub mod manifest;
pub mod native;

pub use adapter::{adapter_for, AppAdapter};
pub use language::{detect_language, AppLanguage};
pub use sbom::generate_sbom;
