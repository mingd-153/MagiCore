use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScaffoldError {
    #[error("Project name '{0}' is invalid: {1}")]
    InvalidName(String, String),

    #[error("Target path already exists: {0}")]
    PathExists(PathBuf),

    #[error("{context}: {source}")]
    IoError {
        context: String,
        source: std::io::Error,
    },

    #[error("Template error: {0}")]
    Template(String),

    #[error("Template directory not found: {0}")]
    TemplateNotFound(PathBuf),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("{0}")]
    Generic(String),
}

impl From<std::io::Error> for ScaffoldError {
    fn from(source: std::io::Error) -> Self {
        ScaffoldError::IoError {
            context: "I/O operation".to_string(),
            source,
        }
    }
}

#[derive(Debug, Error)]
pub enum NameValidationError {
    #[error("Name too long (max 214 characters)")]
    TooLong,

    #[error("Name cannot start with '.' or '_'")]
    InvalidStart,

    #[error("Name contains invalid characters (use [a-z0-9-._~])")]
    InvalidCharacters,

    #[error("Name is empty")]
    Empty,
}
