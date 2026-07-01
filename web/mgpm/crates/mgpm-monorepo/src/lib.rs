//! Monorepo task graph system for MGPM.
//!
//! Builds a dependency graph from workspace script configurations (`ScriptConfig`),
//! performs topological sort via Kahn's algorithm, and executes tasks with
//! configurable parallelism.
//!
//! # Architecture
//!
//! 1. [`TaskGraph::new`] / [`TaskGraph::new_multi`] — builds task nodes from
//!    filtered packages × script names, resolves `depends-on` directives
//! 2. [`TaskGraph::levels`] — groups tasks by topological depth (same level
//!    = no inter-dependency, can run in parallel)
//! 3. [`TaskExecutor`] — executes tasks level-by-level with tokio semaphore
//!
//! # Dependency Syntax
//!
//! | Pattern | Meaning |
//! |---------|---------|
//! | `^build` | Wait for ALL dependency packages' `build` script |
//! | `build` | Wait for this package's own `build` script |
//! | `pkg#build` | Wait for a specific package's `build` script |

pub mod config;
pub mod error;
pub mod executor;
pub mod task_graph;

pub use config::MonorepoConfig;
pub use error::TaskGraphError;
pub use executor::{TaskExecutor, TaskReport, TaskResult, TaskStatus};
pub use task_graph::{TaskGraph, TaskId, TaskNode};
