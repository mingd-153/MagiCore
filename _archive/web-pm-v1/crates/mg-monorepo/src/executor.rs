use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{error, info};

use crate::error::TaskGraphError;
use crate::task_graph::{TaskGraph, TaskId, TaskNode};

/// Result status of a single task execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task completed with exit code 0.
    Success,
    /// Task failed (non-zero exit code or execution error).
    Failed,
    /// Task result was served from cache (T3.7).
    Cached,
    /// Task was skipped (empty command or filtered out).
    Skipped,
}

/// Result of executing a single task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub id: TaskId,
    pub status: TaskStatus,
    pub duration: Duration,
    pub exit_code: Option<i32>,
    pub output: String,
}

/// Aggregate execution report for a full task graph run.
#[derive(Debug, Clone)]
pub struct TaskReport {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub cached: usize,
    pub duration: Duration,
    pub results: Vec<TaskResult>,
}

/// Placeholder for cache engine integration (T3.7).
///
/// Currently a no-op stub. When T3.7 is implemented, this will check/save
/// task outputs based on input hashing.
#[derive(Debug, Clone)]
pub struct CacheEngine;

impl CacheEngine {
    /// Check if a task's result is cached. Always returns `None` (not cached) for now.
    pub fn check(&self, _task: &TaskNode) -> Option<TaskResult> {
        None
    }

    /// Save a task's result to cache. No-op for now.
    pub fn save(&self, _task: &TaskNode, _result: &TaskResult) {}
}

/// Executes tasks from a [`TaskGraph`] with configurable parallelism.
///
/// # Flow
/// 1. Groups tasks by topological level
/// 2. Within each level, spawns tasks concurrently (bounded by semaphore)
/// 3. If `fail_fast`, stops at first failure
/// 4. Persistent tasks (dev servers) are launched but not waited on
#[derive(Debug)]
pub struct TaskExecutor {
    graph: TaskGraph,
    cache: Option<CacheEngine>,
    parallelism: usize,
    fail_fast: bool,
    /// Max seconds to wait for a semaphore slot before error.
    semaphore_timeout_secs: u64,
}

impl TaskExecutor {
    pub fn new(graph: TaskGraph) -> Self {
        Self {
            graph,
            cache: None,
            parallelism: 1,
            fail_fast: true,
            semaphore_timeout_secs: 300,
        }
    }

    /// Set the maximum number of concurrent tasks (default 1).
    pub fn with_parallelism(mut self, n: usize) -> Self {
        if n > 0 {
            self.parallelism = n;
        }
        self
    }

    /// Enable fail-fast (stop on first error) or continue-on-error.
    pub fn with_fail_fast(mut self, yes: bool) -> Self {
        self.fail_fast = yes;
        self
    }

    /// Attach a cache engine for caching task results (T3.7 integration).
    pub fn with_cache(mut self, cache: CacheEngine) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Set semaphore acquire timeout in seconds (default 300).
    pub fn with_semaphore_timeout(mut self, secs: u64) -> Self {
        if secs > 0 {
            self.semaphore_timeout_secs = secs;
        }
        self
    }

    /// Execute all tasks in topological order (shortcut for
    /// [`execute_level_by_level`]).
    pub async fn execute(&self) -> Result<TaskReport, TaskGraphError> {
        self.execute_level_by_level().await
    }

    /// Execute tasks level by level. Tasks within the same level run
    /// concurrently, bounded by the semaphore.
    pub async fn execute_level_by_level(&self) -> Result<TaskReport, TaskGraphError> {
        let start = std::time::Instant::now();
        let mut results: Vec<TaskResult> = Vec::new();
        let semaphore = Arc::new(Semaphore::new(self.parallelism));
        let timeout_secs = self.semaphore_timeout_secs;

        for level in self.graph.levels() {
            let mut handles = Vec::with_capacity(level.len());

            for task_id in level {
                let Some(node) = self.graph.get_node(task_id).cloned() else {
                    continue;
                };
                let sem = Arc::clone(&semaphore);
                let cache = self.cache.clone();
                let is_persistent = node.config.persistent;

                handles.push(tokio::spawn(async move {
                    let _permit = timeout(Duration::from_secs(timeout_secs), sem.acquire())
                        .await
                        .map_err(|_| TaskGraphError::SemaphoreTimeout(timeout_secs))?
                        .map_err(|_| TaskGraphError::Internal("semaphore closed".to_string()))?;

                    if is_persistent {
                        return execute_persistent(node).await;
                    }

                    let task_id = node.id.clone();
                    if let Some(ref engine) = cache {
                        if let Some(cached) = engine.check(&node) {
                            return Ok(cached);
                        }
                    }

                    let result = execute_single(node).await?;

                    if let Some(ref engine) = cache {
                        let n = TaskNode {
                            id: task_id,
                            package: String::new(),
                            script_command: String::new(),
                            config: mg_core::ScriptConfig::default(),
                        };
                        engine.save(&n, &result);
                    }

                    Ok(result)
                }));
            }

            for handle in handles {
                let result = handle
                    .await
                    .map_err(|e| TaskGraphError::Internal(format!("task join error: {e}")))?
                    .unwrap_or_else(|e| TaskResult {
                        id: TaskId::new("unknown", "unknown"),
                        status: TaskStatus::Failed,
                        duration: Duration::default(),
                        exit_code: None,
                        output: e.to_string(),
                    });

                if self.fail_fast && result.status == TaskStatus::Failed {
                    let elapsed = start.elapsed();
                    return Ok(TaskReport {
                        total: self.graph.task_count(),
                        success: results
                            .iter()
                            .filter(|r| r.status == TaskStatus::Success)
                            .count(),
                        failed: results
                            .iter()
                            .filter(|r| r.status == TaskStatus::Failed)
                            .count()
                            + 1,
                        cached: results
                            .iter()
                            .filter(|r| r.status == TaskStatus::Cached)
                            .count(),
                        duration: elapsed,
                        results: {
                            let mut all = results;
                            all.push(result);
                            all
                        },
                    });
                }

                results.push(result);
            }
        }

        let elapsed = start.elapsed();
        let success = results
            .iter()
            .filter(|r| r.status == TaskStatus::Success)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == TaskStatus::Failed)
            .count();
        let cached = results
            .iter()
            .filter(|r| r.status == TaskStatus::Cached)
            .count();

        Ok(TaskReport {
            total: self.graph.task_count(),
            success,
            failed,
            cached,
            duration: elapsed,
            results,
        })
    }
}

/// Execute a regular (non-persistent) task via `sh -c`.
async fn execute_single(node: TaskNode) -> Result<TaskResult, TaskGraphError> {
    let start = std::time::Instant::now();

    if node.script_command.is_empty() {
        return Ok(TaskResult {
            id: node.id,
            status: TaskStatus::Skipped,
            duration: Duration::default(),
            exit_code: Some(0),
            output: String::new(),
        });
    }

    let output = Command::new("sh")
        .arg("-c")
        .arg(&node.script_command)
        .output()
        .await
        .map_err(|e| TaskGraphError::ProcessError(format!("failed to spawn process: {e}")))?;

    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else {
        format!("{stdout}\n{stderr}")
    };

    let exit_code = output.status.code();
    let status = if output.status.success() {
        TaskStatus::Success
    } else {
        TaskStatus::Failed
    };

    match status {
        TaskStatus::Failed => {
            error!(task = %node.id, code = ?exit_code, "task failed");
        }
        TaskStatus::Success => {
            info!(task = %node.id, "task completed");
        }
        _ => {}
    }

    Ok(TaskResult {
        id: node.id,
        status,
        duration: elapsed,
        exit_code,
        output: combined,
    })
}

/// Execute a persistent task (e.g., dev server).
///
/// Persistent tasks are spawned and tracked, but the executor does not wait
/// for them to finish. They are reported as `Success` immediately after launch.
async fn execute_persistent(node: TaskNode) -> Result<TaskResult, TaskGraphError> {
    let start = std::time::Instant::now();

    if node.script_command.is_empty() {
        return Ok(TaskResult {
            id: node.id,
            status: TaskStatus::Skipped,
            duration: Duration::default(),
            exit_code: Some(0),
            output: String::new(),
        });
    }

    let child = Command::new("sh")
        .arg("-c")
        .arg(&node.script_command)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            TaskGraphError::ProcessError(format!("failed to spawn persistent task: {e}"))
        })?;

    info!(task = %node.id, pid = ?child.id(), "persistent task started");

    let elapsed = start.elapsed();
    Ok(TaskResult {
        id: node.id,
        status: TaskStatus::Success,
        duration: elapsed,
        exit_code: None,
        output: format!("persistent task started (pid: {:?})", child.id()),
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::task_graph::test as tg_test;

    #[tokio::test]
    async fn test_executor_runs_tasks() {
        let (_ws, pg, members) = tg_test::build_chain_graph(&["test-pkg"]);
        let mut custom_scripts = std::collections::HashMap::new();
        custom_scripts.insert(
            "echo".to_string(),
            mg_core::ScriptConfig {
                command: Some("echo hello".to_string()),
                depends_on: vec![],
                cache: true,
                inputs: vec![],
                outputs: vec![],
                persistent: false,
            },
        );

        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "echo",
            &custom_scripts,
        )
        .unwrap();

        let executor = TaskExecutor::new(graph).with_parallelism(4);
        let report = executor.execute().await.unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(report.success, 1);
        assert_eq!(report.failed, 0);
    }

    #[tokio::test]
    async fn test_executor_skip_empty_command() {
        let (_ws, pg, members) = tg_test::build_chain_graph(&["test-pkg"]);
        let mut custom_scripts = std::collections::HashMap::new();
        custom_scripts.insert(
            "noop".to_string(),
            mg_core::ScriptConfig {
                command: None,
                depends_on: vec![],
                cache: true,
                inputs: vec![],
                outputs: vec![],
                persistent: false,
            },
        );

        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "noop",
            &custom_scripts,
        )
        .unwrap();

        let executor = TaskExecutor::new(graph);
        let report = executor.execute().await.unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(report.success, 0);
        assert_eq!(report.cached, 0);
    }

    #[tokio::test]
    async fn test_executor_continue_on_error() {
        let (_ws, pg, members) = tg_test::build_chain_graph(&["pkg-a", "pkg-b"]);
        let mut scripts = std::collections::HashMap::new();
        scripts.insert(
            "fail".to_string(),
            mg_core::ScriptConfig {
                command: Some("false".to_string()),
                depends_on: vec![],
                cache: false,
                inputs: vec![],
                outputs: vec![],
                persistent: false,
            },
        );

        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "fail",
            &scripts,
        )
        .unwrap();

        let executor = TaskExecutor::new(graph)
            .with_parallelism(4)
            .with_fail_fast(false);
        let report = executor.execute().await.unwrap();
        assert_eq!(report.total, 2);
        assert_eq!(report.failed, 2);
        assert_eq!(report.success, 0);
    }

    #[tokio::test]
    async fn test_executor_fail_fast() {
        let (_ws, pg, members) = tg_test::build_chain_graph(&["pkg-a", "pkg-b"]);
        let mut scripts = std::collections::HashMap::new();
        scripts.insert(
            "fail".to_string(),
            mg_core::ScriptConfig {
                command: Some("false".to_string()),
                depends_on: vec![],
                cache: false,
                inputs: vec![],
                outputs: vec![],
                persistent: false,
            },
        );

        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "fail",
            &scripts,
        )
        .unwrap();

        let executor = TaskExecutor::new(graph)
            .with_parallelism(4)
            .with_fail_fast(true);
        let report = executor.execute().await.unwrap();
        assert!(report.failed >= 1);
    }

    #[tokio::test]
    async fn test_executor_persistent_task() {
        let (_ws, pg, members) = tg_test::build_chain_graph(&["test-pkg"]);
        let mut scripts = std::collections::HashMap::new();
        scripts.insert(
            "serve".to_string(),
            mg_core::ScriptConfig {
                command: Some("echo 'persistent started'".to_string()),
                depends_on: vec![],
                cache: false,
                inputs: vec![],
                outputs: vec![],
                persistent: true,
            },
        );

        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "serve",
            &scripts,
        )
        .unwrap();

        let executor = TaskExecutor::new(graph);
        let report = executor.execute().await.unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(report.success, 1);
    }

    #[tokio::test]
    async fn test_executor_level_execution_order() {
        let (_ws, pg, members) = tg_test::build_caret_graph();
        let scripts = tg_test::make_scripts_config("build", vec!["^build"]);

        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        )
        .unwrap();

        assert_eq!(graph.levels().len(), 3);
        let executor = TaskExecutor::new(graph).with_parallelism(4);
        let report = executor.execute().await.unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.success, 3);
    }

    #[test]
    fn test_cache_engine_stub() {
        let engine = CacheEngine;
        let task = TaskNode {
            id: TaskId::new("pkg", "build"),
            package: "pkg".to_string(),
            script_command: "echo hi".to_string(),
            config: mg_core::ScriptConfig::default(),
        };
        assert!(engine.check(&task).is_none());
    }

    #[test]
    fn test_executor_builder_methods() {
        let (_ws, pg, members) = tg_test::build_chain_graph(&["x"]);
        let scripts = tg_test::make_scripts_config("build", vec![]);
        let graph = crate::TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        )
        .unwrap();

        let _ex = TaskExecutor::new(graph)
            .with_parallelism(8)
            .with_fail_fast(false)
            .with_cache(CacheEngine)
            .with_semaphore_timeout(600);
    }
}
