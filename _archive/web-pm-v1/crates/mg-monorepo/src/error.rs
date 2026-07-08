use thiserror::Error;

/// Errors that can occur during task graph operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskGraphError {
    /// The requested script name was not found in workspace config.
    #[error("script '{0}' not found in workspace config")]
    ScriptNotFound(String),

    /// A package does not have the referenced script.
    #[error("package '{0}' has no script named '{1}'")]
    MissingScript(String, String),

    /// Circular dependency detected among tasks.
    #[error("circular task dependency detected: {0}")]
    CircularDependency(String),

    /// Task execution returned a non-zero exit code or failed.
    #[error("task execution failed: {0}")]
    ExecutionFailed(String),

    /// Error spawning or communicating with a child process.
    #[error("child process error: {0}")]
    ProcessError(String),

    /// Semaphore acquire timed out (concurrency slot unavailable).
    #[error("semaphore acquire timed out after {0}s")]
    SemaphoreTimeout(u64),

    /// Internal / unexpected error.
    #[error("{0}")]
    Internal(String),
}
