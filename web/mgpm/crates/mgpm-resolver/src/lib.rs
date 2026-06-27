//! MGPM Resolver Crate
//!
//! PubGrub-based dependency resolver with catalog, workspace, and override support.

pub mod solver;
pub mod version;

pub use solver::{Resolution, Resolver, SolveError, SolveResult, DependencyProvider};
pub use solver::pubgrub::{Term, Incompatibility, Cause, PubGrubSolver, SolveError as PubGrubSolveError};
pub use version::VersionSet;

pub type ResolvedDependency = solver::ResolvedDep;

#[derive(Debug, Clone)]
pub struct ResolveError(String);

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ResolveError {}

impl From<String> for ResolveError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ResolveError {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<solver::SolveError> for ResolveError {
    fn from(e: solver::SolveError) -> Self {
        Self(e.message)
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_set_union() {
        use mgpm_core::{Version, VersionRange};
        
        let a = VersionSet::range(VersionRange::parse("^1.0.0").unwrap());
        let b = VersionSet::range(VersionRange::parse("^2.0.0").unwrap());
        let union = a.union(&b);
        
        assert!(union.contains(&Version::parse("1.5.0").unwrap()));
        assert!(union.contains(&Version::parse("2.5.0").unwrap()));
        assert!(!union.contains(&Version::parse("0.9.0").unwrap()));
    }

    #[test]
    fn test_version_set_intersection() {
        use mgpm_core::{Version, VersionRange};
        
        let a = VersionSet::range(VersionRange::parse(">=1.0.0").unwrap());
        let b = VersionSet::range(VersionRange::parse("<2.0.0").unwrap());
        let inter = a.intersection(&b);
        
        assert!(inter.contains(&Version::parse("1.5.0").unwrap()));
        assert!(!inter.contains(&Version::parse("0.9.0").unwrap()));
        assert!(!inter.contains(&Version::parse("2.0.0").unwrap()));
    }
}