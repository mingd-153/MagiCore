use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScaffoldError {
    #[error("Project name '{0}' is invalid: {1}")]
    InvalidName(String, String),

    #[error("Target path already exists: {0}")]
    PathExists(PathBuf),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Template directory not found: {0}")]
    TemplateNotFound(PathBuf),

    #[error("Internal error: {0}")]
    Internal(String),
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
