//! mgc-app-adapter — mobile app ecosystem adapter for MagiCore.
//! App core hỗ trợ Flutter/Kotlin/Swift/React Native/ObjC/Multi qua module rõ trách nhiệm.

mod adapter;
mod language;
mod sbom;

pub use adapter::{adapter_for, AppAdapter};
pub use language::{detect_language, AppLanguage};
pub use sbom::generate_sbom;
