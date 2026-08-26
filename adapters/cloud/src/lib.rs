#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-cloud-adapter — cloud ecosystem adapter for MagiCore.
//! Cloud core hỗ trợ CDK/Pulumi/Terraform/Cloudflare qua module rõ trách nhiệm.

mod adapter;
mod cloud_type;
mod sbom;
mod tooling;

pub mod deploy;
pub mod install;
pub mod scaffold;

pub use adapter::{adapter_for, CloudAdapter};
pub use cloud_type::{detect_type, CloudType};
pub use sbom::generate_sbom;
