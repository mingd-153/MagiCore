//! MegaGate shared types, traits, and errors
//!
//! This crate is the foundation for ALL MegaGate adapters:
//! - Web (npm), Game (Bevy/Unity), AI (PyPI), Cloud, IoT
//!
//! All other crates depend on this one.

pub mod package;
pub mod version;
pub mod error;
pub mod manifest;
pub mod adapter;
pub mod ecosystem;

pub use package::{PackageName, PackageId, DependencySpec, VersionRange};
pub use version::{Version, SemVerError};
pub use package::VersionReq;
pub use error::{MgError, MgResult};
pub use manifest::Manifest;
pub use adapter::PackageAdapter;
pub use ecosystem::Ecosystem;
