//! mgc-ai-adapter — AI ecosystem adapter for MagiCore.
//! AI core hỗ trợ Python Agent và MCP Server qua module rõ trách nhiệm.

mod adapter;
mod framework;
mod sbom;

pub use adapter::{adapter_for, AiAdapter};
pub use framework::{detect_framework, AiFramework};
pub use sbom::generate_sbom;
