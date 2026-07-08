pub mod installer;

pub use installer::{
    InstallError, InstallOptions, InstallPhase, InstallProgress, InstallResult, Installer,
    JsonlLogger,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("install error: {0}")]
    Install(#[from] InstallError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Install(InstallError::StoreError(s))
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self::Install(InstallError::StoreError(s.to_string()))
    }
}
