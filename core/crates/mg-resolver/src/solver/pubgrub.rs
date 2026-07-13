use crate::version::VersionSet;
use mg_types::{PackageName, Version};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    Positive(PackageName, VersionSet),
    Negative(PackageName, VersionSet),
}

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

#[derive(Debug, Clone)]
pub enum Cause {
    Dependency {
        package: PackageName,
        version: Version,
        dep_package: PackageName,
        dep_version_set: VersionSet,
    },
    Conflict {
        incompatibility1: Box<Incompatibility>,
        incompatibility2: Box<Incompatibility>,
    },
}

#[derive(Debug, Default)]
pub struct PubGrubSolver {
    incompatibilities: Vec<Incompatibility>,
    assignment: Vec<(PackageName, Version, VersionSet)>,
    decision_level: usize,
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

    pub fn add_incompatibility(&mut self, inc: Incompatibility) {
        self.incompatibilities.push(inc);
    }

    #[allow(clippy::result_large_err)]
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
            Term::Positive(name, vs) => self
                .decisions
                .get(name)
                .map(|v| vs.contains(v))
                .unwrap_or(false),
            Term::Negative(name, vs) => self
                .decisions
                .get(name)
                .map(|v| !vs.contains(v))
                .unwrap_or(true),
        }
    }

    #[allow(clippy::result_large_err)]
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

            for decision in new_decisions {
                propagated.push(decision.clone());
                self.apply_decision(&decision);
            }
        }

        for inc in &self.incompatibilities {
            if inc.terms.iter().all(|t| self.term_satisfied(t)) {
                return Err(inc.clone());
            }
        }

        Ok(propagated)
    }

    fn find_unit_propagation(&self, inc: &Incompatibility) -> Option<Decision> {
        let unsatisfied: Vec<_> = inc
            .terms
            .iter()
            .filter(|t| !self.term_satisfied(t))
            .collect();

        if unsatisfied.len() == 1 {
            let term = unsatisfied[0];
            match term {
                Term::Positive(name, vs) => Some(Decision::Assigned(
                    name.clone(),
                    vs.satisfying_version()
                        .unwrap_or_else(|| Version::new(0, 0, 0)),
                    vs.clone(),
                )),
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
                self.assignment
                    .push((name.clone(), version.clone(), vs.clone()));
                self.decision_level += 1;
            }
            Decision::Backtracked(name) => {
                self.decisions.remove(name);
                self.assignment.retain(|(n, _, _)| n != name);
                self.decision_level = self.decision_level.saturating_sub(1);
            }
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn solve(
        &mut self,
        wanted: &[(PackageName, VersionSet)],
    ) -> Result<HashMap<PackageName, Version>, SolveError> {
        for (name, vs) in wanted {
            self.add_incompatibility(Incompatibility::new(vec![Term::Positive(
                name.clone(),
                vs.clone(),
            )]));
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
                    if !self.backtrack(&conflict)? {
                        return Err(SolveError::Unsatisfiable(conflict));
                    }
                }
            }
        }
    }

    fn is_complete(&self, wanted: &[(PackageName, VersionSet)]) -> bool {
        wanted
            .iter()
            .all(|(name, _)| self.decisions.contains_key(name))
    }

    #[allow(clippy::result_large_err)]
    fn choose_package(&self, wanted: &[(PackageName, VersionSet)]) -> Result<Decision, SolveError> {
        for (name, vs) in wanted {
            if !self.decisions.contains_key(name) {
                if let Some(version) = vs.satisfying_version() {
                    return Ok(Decision::Assigned(name.clone(), version, vs.clone()));
                }
            }
        }

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

    #[allow(clippy::result_large_err)]
    fn backtrack(&mut self, _conflict: &Incompatibility) -> Result<bool, SolveError> {
        if let Some(last) = self.assignment.pop() {
            self.decisions.remove(&last.0);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Debug, Clone)]
pub enum DerivationTree {
    Root(Vec<DerivationTree>),
    Dependency {
        package: PackageName,
        version: Version,
        dep: Box<DerivationTree>,
    },
    Conflict {
        left: Box<DerivationTree>,
        right: Box<DerivationTree>,
    },
    NoVersion {
        package: PackageName,
        constraint: VersionSet,
    },
}

impl std::fmt::Display for DerivationTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DerivationTree::Root(children) => {
                for child in children {
                    writeln!(f, "  {child}")?;
                }
                Ok(())
            }
            DerivationTree::Dependency {
                package,
                version,
                dep,
            } => {
                write!(f, "{package}@{version} depends on {dep}")
            }
            DerivationTree::Conflict { left, right } => {
                write!(f, "conflict: {left} vs {right}")
            }
            DerivationTree::NoVersion {
                package,
                constraint,
            } => {
                write!(f, "no version of {package} satisfies {constraint}")
            }
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SolveError {
    #[error("unsatisfiable constraints")]
    Unsatisfiable(Incompatibility),
    #[error("no package found to satisfy constraints")]
    NoPackageFound,
}

impl From<Incompatibility> for SolveError {
    fn from(inc: Incompatibility) -> Self {
        SolveError::Unsatisfiable(inc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incompatibility() {
        let inc = Incompatibility::new(vec![
            Term::Positive(PackageName::new("react").unwrap(), VersionSet::any()),
            Term::Negative(
                PackageName::new("react").unwrap(),
                VersionSet::range(mg_types::VersionRange::parse("^18.0.0").unwrap()),
            ),
        ]);
        assert_eq!(inc.terms.len(), 2);
    }
}
