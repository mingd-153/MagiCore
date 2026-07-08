pub mod cache;
pub mod graph;
pub mod solver;
pub mod version;

pub use cache::RegistryCache;
pub use graph::DependencyGraph;
pub use solver::{
    check_dependency_confusion, DepInfo, DependencyProvider, ResolvedDep, Resolution, Resolver,
    SolveError, SolveResult,
};
pub use version::VersionSet;

pub use solver::pubgrub::{Cause, DerivationTree, Incompatibility, PubGrubSolver, Term};

#[derive(Debug, Clone)]
pub struct ResolveError(String);

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ResolveError {}

impl From<String> for ResolveError {
    fn from(s: String) -> Self { Self(s) }
}

impl From<&str> for ResolveError {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}

impl From<SolveError> for ResolveError {
    fn from(e: SolveError) -> Self { Self(e.message) }
}
