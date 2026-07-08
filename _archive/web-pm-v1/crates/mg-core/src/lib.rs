#![allow(unsafe_op_in_unsafe_fn)]
//! MegaGate Package Manager - Core Types and Primitives
//!
//! This crate contains the foundational types used across all mg components:
//! - Package identity (names, versions, IDs)
//! - Semantic versioning
//! - Package resolution and protocols
//! - Configuration schemas
//! - Error types
//! - Global allocator and tracing setup

pub mod alloc;
pub mod package;
pub mod semver;
pub mod protocol;
pub mod config;
pub mod error;
pub mod logging;
pub mod cffi;

pub use package::*;
pub use semver::{Version, SemVerError, IntegrityHash};
pub use protocol::*;
pub use config::*;
pub use error::*;
pub use logging::{init_tracing, TracingConfig};

pub mod arena;
pub mod platform;
