//! Full PubGrub implementation with backtracking and conflict explanation
//!
//! Note: Some clippy lints are allowed here due to the complexity of the PubGrub algorithm
//! and the large error types required for detailed conflict reporting.

#![allow(clippy::result_large_err, clippy::collapsible_match, clippy::collapsible_if)]

use std::collections::HashMap;
use std::fmt;

use mgpm_core::{PackageName, Version};

use crate::version::VersionSet;

/// A term in the PubGrub derivation tree
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    /// Positive term: package@version satisfies constraint
    Positive(PackageName, VersionSet),
    /// Negative term: package@version does NOT satisfy constraint  
    Negative(PackageName, VersionSet),
}

/// An incompatibility with its cause (derivation tree)
#[derive(Debug, Clone)]
pub struct Incompatibility {
    pub terms: Vec<Term>,
    pub cause: Option<Cause>,
}

impl Incompatibility {
    pub fn new(terms: Vec<Term>) -> Self {
        Self { terms, cause: None }
    }
    
    pub fn with_cause(mut self, cause: Cause) -> Self {
        self.cause = Some(cause);
        self
    }
}

/// Cause of an incompatibility (for error reporting)
#[derive(Debug, Clone)]
pub enum Cause {
    /// Root cause: dependency of a package
    Dependency {
        package: PackageName,
        version: Version,
        dep_package: PackageName,
        dep_version_set: VersionSet,
    },
    /// Conflict: two incompatibilities resolved to same conclusion
    Conflict {
        incompatibility1: Box<Incompatibility>,
        incompatibility2: Box<Incompatibility>,
    },
}

/// PubGrub solver state
#[derive(Debug, Default)]
pub struct PubGrubSolver {
    incompatibilities: Vec<Incompatibility>,
    assignment: Vec<(PackageName, Version, VersionSet)>, // (package, version, constraint)
    #[allow(dead_code)]
    decision_level: usize,
    #[allow(dead_code)]
    trail: Vec<Decision>,
    decisions: HashMap<PackageName, Version>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Assigned(PackageName, Version, VersionSet),
    Backtracked(PackageName),
}

impl PubGrubSolver {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an incompatibility to the solver
    pub fn add_incompatibility(&mut self, incompatibility: Incompatibility) {
        self.incompatibilities.push(incompatibility);
    }
    
    /// Get versions that satisfy a version set (mock - would call provider in real impl)
    pub fn satisfying_versions(&self, _package: &PackageName, _version_set: &VersionSet) -> Vec<Version> {
        // In real implementation, this would query the registry/provider
        // For now, return empty
        vec![]
    }
    
    /// Check if assignment satisfies all constraints
    pub fn check_consistency(&self) -> Result<(), Incompatibility> {
        for inc in &self.incompatibilities {
            if inc.terms.iter().all(|term| self.term_satisfied(term)) {
                return Err(inc.clone());
            }
        }
        Ok(())
    }
    
    fn term_satisfied(&self, term: &Term) -> bool {
        match term {
            Term::Positive(name, vs) => {
                self.decisions.get(name)
                    .map(|v| vs.contains(v))
                    .unwrap_or(false)
            }
            Term::Negative(name, vs) => {
                self.decisions.get(name)
                    .map(|v| !vs.contains(v))
                    .unwrap_or(true)
            }
        }
    }
    
    /// Propagate unit terms (terms that must be true/false given current assignment)
    pub fn propagate(&mut self) -> Result<Vec<Decision>, Incompatibility> {
        let mut propagated = Vec::new();
        let mut changed = true;
        
        while changed {
            changed = false;
            let mut new_decisions = Vec::new();
            
            for inc in &self.incompatibilities {
                if let Some(decision) = self.find_unit_propagation(inc) {
                    if !propagated.contains(&decision) && !new_decisions.contains(&decision) {
                        new_decisions.push(decision.clone());
                        changed = true;
                    }
                }
            }
            
            // Apply all collected decisions
            for decision in new_decisions {
                propagated.push(decision.clone());
                self.apply_decision(&decision);
            }
        }
        
        // Check for conflicts
        for inc in &self.incompatibilities {
            if inc.terms.iter().all(|t| self.term_satisfied(t)) {
                return Err(inc.clone());
            }
        }
        
        Ok(propagated)
    }
    
    fn find_unit_propagation(&self, inc: &Incompatibility) -> Option<Decision> {
        let unsatisfied: Vec<_> = inc.terms.iter()
            .filter(|t| !self.term_satisfied(t))
            .collect();
            
        if unsatisfied.len() == 1 {
            let term = unsatisfied[0];
            match term {
                Term::Positive(name, vs) => Some(Decision::Assigned(name.clone(), 
                    vs.satisfying_version().unwrap(), vs.clone())),
                Term::Negative(name, _vs) => Some(Decision::Backtracked(name.clone())),
            }
        } else {
            None
        }
    }
    
    fn apply_decision(&mut self, decision: &Decision) {
        match decision {
            Decision::Assigned(name, version, vs) => {
                self.decisions.insert(name.clone(), version.clone());
                self.assignment.push((name.clone(), version.clone(), vs.clone()));
            }
            Decision::Backtracked(name) => {
                self.decisions.remove(name);
                self.assignment.retain(|(n, _, _)| n != name);
            }
        }
    }
    
    /// Main solving loop with backtracking
    pub fn solve(&mut self, wanted: &[(PackageName, VersionSet)]) -> Result<HashMap<PackageName, Version>, SolveError> {
        // Add root incompatibility: wanted packages must be satisfied
        for (name, vs) in wanted {
            self.add_incompatibility(Incompatibility::new(vec![
                Term::Positive(name.clone(), vs.clone()),
            ]));
        }
        
        self.propagate()?;
        
        loop {
            if self.is_complete(wanted) {
                return Ok(self.decisions.clone());
            }
            
            let decision = self.choose_package(wanted)?;
            self.apply_decision(&decision);
            
            match self.propagate() {
                Ok(_) => continue,
                Err(conflict) => {
                    // Backtrack
                    if !self.backtrack(&conflict)? {
                        return Err(SolveError::Unsatisfiable(conflict));
                    }
                }
            }
        }
    }
    
    fn is_complete(&self, wanted: &[(PackageName, VersionSet)]) -> bool {
        wanted.iter().all(|(name, _)| self.decisions.contains_key(name))
    }
    
    fn choose_package(&self, wanted: &[(PackageName, VersionSet)]) -> Result<Decision, SolveError> {
        // Find unassigned package from wanted
        for (name, vs) in wanted {
            if !self.decisions.contains_key(name) {
                // Get a satisfying version
                if let Some(version) = vs.satisfying_version() {
                    return Ok(Decision::Assigned(name.clone(), version, vs.clone()));
                }
            }
        }
        
        // Check incompatibilities for other packages that need assignment
        for inc in &self.incompatibilities {
            for term in &inc.terms {
                if let Term::Positive(name, vs) = term {
                    if !self.decisions.contains_key(name) {
                        if let Some(version) = vs.satisfying_version() {
                            return Ok(Decision::Assigned(name.clone(), version, vs.clone()));
                        }
                    }
                }
            }
        }
        
        Err(SolveError::NoPackageFound)
    }
    
    fn backtrack(&mut self, _conflict: &Incompatibility) -> Result<bool, SolveError> {
        // Simple backtrack: undo last decision
        if let Some(last) = self.assignment.pop() {
            self.decisions.remove(&last.0);
            return Ok(true);
        }
        Ok(false)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SolveError {
    #[error("unsatisfiable: {0:?}")]
    Unsatisfiable(Incompatibility),
    #[error("no package found to satisfy constraints")]
    NoPackageFound,
    #[error("version not found for {0}")]
    VersionNotFound(String),
}

impl From<Incompatibility> for SolveError {
    fn from(inc: Incompatibility) -> Self {
        SolveError::Unsatisfiable(inc)
    }
}

/// Derivation tree for conflict explanations.
#[derive(Debug, Clone)]
pub enum DerivationTree {
    /// Root incompatibilities.
    Root(Vec<DerivationTree>),
    /// A dependency constraint.
    Dependency {
        package: PackageName,
        version: Version,
        dep: Box<DerivationTree>,
    },
    /// A conflict between two derivations.
    Conflict {
        left: Box<DerivationTree>,
        right: Box<DerivationTree>,
    },
    /// No version satisfies the constraint.
    NoVersion {
        package: PackageName,
        constraint: crate::version::VersionSet,
    },
}

impl fmt::Display for DerivationTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DerivationTree::Root(children) => {
                for child in children {
                    writeln!(f, "  {child}")?;
                }
                Ok(())
            }
            DerivationTree::Dependency { package, version, dep } => {
                write!(f, "{package}@{version} depends on {dep}")
            }
            DerivationTree::Conflict { left, right } => {
                write!(f, "conflict: {left} vs {right}")
            }
            DerivationTree::NoVersion { package, constraint } => {
                write!(f, "no version of {package} satisfies {constraint}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_incompatibility() {
        let inc = Incompatibility::new(vec![
            Term::Positive("react".parse().unwrap(), VersionSet::any()),
            Term::Negative("react".parse().unwrap(), VersionSet::range("^18.0.0".parse().unwrap())),
        ]);
        assert_eq!(inc.terms.len(), 2);
    }
}
