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
