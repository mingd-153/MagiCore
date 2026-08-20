#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod adapter;
pub mod ecosystem;
pub mod error;
pub mod manifest;
pub mod package;
pub mod patch;
pub mod publish;
pub mod version;

pub use adapter::*;
pub use ecosystem::Ecosystem;
pub use error::{MgError, MgResult};
pub use manifest::Manifest;
pub use package::{DependencySpec, PackageId, PackageName, VersionRange};
pub use patch::{LockPatch, PatchKind, PatchSpec};
pub use publish::{PublishOptions, PublishSummary};
pub use version::Version;

/// Censor sensitive values in config strings.
/// Replaces common secret/token identifiers with "****".
pub fn censor_secret(input: &str) -> String {
    input
        .replace("token", "****")
        .replace("password", "****")
        .replace("secret", "****")
        .replace("key", "****")
}
