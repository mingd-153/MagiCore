use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskGraphError {
    #[error("script '{0}' not found in workspace config")]
    ScriptNotFound(String),

    #[error("package '{0}' has no script named '{1}'")]
    MissingScript(String, String),

    #[error("circular task dependency detected: {0}")]
    CircularDependency(String),

    #[error("task execution failed: {0}")]
    ExecutionFailed(String),

    #[error("child process error: {0}")]
    ProcessError(String),

    #[error("{0}")]
    Internal(String),
}
