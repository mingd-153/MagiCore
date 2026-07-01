pub mod http;
pub mod registry;

pub use http::{DownloadError, DownloadManager, DownloadRequest, DownloadedPackage};
pub use registry::{
    FileRegistry, WorkspaceRegistry, PackageJsonReader, ParsedPackageJson,
    NpmRegistry, JsrRegistry, GitRegistry, HttpRegistry,
    RegistryClient, RegistryManager, RegistryError,
};
