#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-cloud-adapter — cloud ecosystem adapter for MagiCore.
//! Cloud core hỗ trợ CDK/Pulumi/Terraform/Cloudflare qua module rõ trách nhiệm.

mod adapter;
mod cloud_type;
mod sbom;

pub mod deploy;
pub mod scaffold;

pub use adapter::{CloudAdapter, adapter_for};
pub use cloud_type::{CloudType, detect_type};
pub use sbom::generate_sbom;
