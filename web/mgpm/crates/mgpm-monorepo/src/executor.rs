use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{error, info};

use crate::error::TaskGraphError;
use crate::task_graph::{TaskGraph, TaskId, TaskNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Success,
    Failed,
    Cached,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub id: TaskId,
    pub status: TaskStatus,
    pub duration: Duration,
    pub exit_code: Option<i32>,
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct TaskReport {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub cached: usize,
    pub duration: Duration,
    pub results: Vec<TaskResult>,
}

#[derive(Debug)]
pub struct TaskExecutor {
    graph: TaskGraph,
    parallelism: usize,
    fail_fast: bool,
}

impl TaskExecutor {
    pub fn new(graph: TaskGraph) -> Self {
        Self {
            graph,
            parallelism: 1,
            fail_fast: true,
        }
    }

    pub fn with_parallelism(mut self, n: usize) -> Self {
        if n > 0 {
            self.parallelism = n;
        }
        self
    }

    pub fn with_fail_fast(mut self, yes: bool) -> Self {
        self.fail_fast = yes;
        self
    }

    pub async fn execute(&self) -> Result<TaskReport, TaskGraphError> {
        self.execute_level_by_level().await
    }

    pub async fn execute_level_by_level(&self) -> Result<TaskReport, TaskGraphError> {
        let start = std::time::Instant::now();
        let mut results: Vec<TaskResult> = Vec::new();
        let semaphore = Arc::new(Semaphore::new(self.parallelism));

        for level in self.graph.levels() {
            let mut handles = Vec::with_capacity(level.len());

            for task_id in level {
                let Some(node) = self.graph.get_node(task_id).cloned() else {
                    continue;
                };
                let sem = Arc::clone(&semaphore);

                handles.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    execute_single(node).await
                }));
            }

            for handle in handles {
                let result = handle
                    .await
                    .map_err(|e| TaskGraphError::Internal(format!("task join error: {e}")))?
                    .unwrap_or_else(|e| {
                        let id = TaskId::new("unknown", "unknown");
                        TaskResult {
                            id,
                            status: TaskStatus::Failed,
                            duration: Duration::default(),
                            exit_code: None,
                            output: e.to_string(),
                        }
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
            mgpm_core::ScriptConfig {
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
            mgpm_core::ScriptConfig {
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
}
