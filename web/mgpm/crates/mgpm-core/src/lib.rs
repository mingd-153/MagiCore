//! MegaGate Package Manager - Core Types and Primitives
//!
//! This crate contains the foundational types used across all mgpm components:
//! - Package identity (names, versions, IDs)
//! - Semantic versioning
//! - Package resolution and protocols
//! - Configuration schemas
//! - Error types

pub mod package;
pub mod semver;
pub mod protocol;
pub mod config;
pub mod error;

pub use package::*;
pub use semver::{Version, SemVerError, IntegrityHash};
pub use protocol::*;
pub use config::*;
pub use error::*;