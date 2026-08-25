//! mgc-cicd-adapter — CI/CD ecosystem adapter for MagiCore.
//! CI/CD core hỗ trợ provider đa cloud qua module rõ trách nhiệm.

mod adapter;
mod provider;
mod sbom;

pub use adapter::{adapter_for, CicdAdapter};
pub use provider::{detect_provider, CicdProvider};
pub use sbom::generate_sbom;
