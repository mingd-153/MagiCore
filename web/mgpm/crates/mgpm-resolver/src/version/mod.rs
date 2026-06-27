//! Version range operations and set algebra
//!
//! Implements VersionSet with union, intersection, complement, and containment.

use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};

use mgpm_core::{Version, VersionRange, DependencySpec, PackageName, PackageId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionSet {
    Any,
    Empty,
    Exact(Version),
    Range(VersionRange),
    Union(Vec<VersionSet>),
    Intersection(Vec<VersionSet>),
    Complement(Box<VersionSet>),
}

impl VersionSet {
    pub fn any() -> Self {
        Self::Any
    }

    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn exact(v: Version) -> Self {
        Self::Exact(v)
    }

    pub fn range(r: VersionRange) -> Self {
        Self::Range(r)
    }

    pub fn from_spec(_spec: &mgpm_core::DependencySpec) -> Self {
        Self::Any
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    pub fn contains(&self, version: &Version) -> bool {
        match self {
            Self::Any => true,
            Self::Empty => false,
            Self::Exact(v) => v == version,
            Self::Range(r) => r.contains(version),
            Self::Union(sets) => sets.iter().any(|s| s.contains(version)),
            Self::Intersection(sets) => sets.iter().all(|s| s.contains(version)),
            Self::Complement(s) => !s.contains(version),
        }
    }

    pub fn intersects(&self, other: &VersionSet) -> bool {
        !self.intersection(other).is_empty()
    }

    pub fn union(&self, other: &VersionSet) -> VersionSet {
        match (self, other) {
            (Self::Empty, s) => s.clone(),
            (s, Self::Empty) => s.clone(),
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::Exact(a), Self::Exact(b)) if a == b => self.clone(),
            _ => Self::Union(vec![self.clone(), other.clone()]),
        }
    }

    pub fn intersection(&self, other: &VersionSet) -> VersionSet {
        match (self, other) {
            (Self::Empty, _) | (_, Self::Empty) => Self::Empty,
            (Self::Any, s) | (s, Self::Any) => s.clone(),
            (Self::Exact(a), Self::Exact(b)) if a == b => self.clone(),
            (Self::Exact(v), Self::Range(r)) | (Self::Range(r), Self::Exact(v)) => {
                if r.contains(v) {
                    Self::Exact(v.clone())
                } else {
                    Self::Empty
                }
            }
            (Self::Range(a), Self::Range(b)) => {
                Self::Range(a.intersection(b))
            }
            _ => Self::Intersection(vec![self.clone(), other.clone()]),
        }
    }

    pub fn complement(&self) -> VersionSet {
        match self {
            Self::Empty => Self::Any,
            Self::Any => Self::Empty,
            Self::Complement(s) => (**s).clone(),
            _ => Self::Complement(Box::new(self.clone())),
        }
    }

    pub fn subtract(&self, other: &VersionSet) -> VersionSet {
        self.intersection(&other.complement())
    }
    
    pub fn satisfying_version(&self) -> Option<Version> {
        match self {
            Self::Any => Some(Version::new(0, 0, 0)), // lowest
            Self::Empty => None,
            Self::Exact(v) => Some(v.clone()),
            Self::Range(r) => r.satisfying_version(),
            Self::Union(sets) => {
                // Return highest version from any set
                sets.iter().filter_map(|s| s.satisfying_version()).max()
            }
            Self::Intersection(sets) => {
                // Return first version that satisfies all
                sets.iter().filter_map(|s| s.satisfying_version()).find(|v| {
                    sets.iter().all(|s| s.contains(v))
                })
            }
            Self::Complement(s) => {
                // Return first version not in complement
                s.satisfying_version().and_then(|v| {
                    if !self.contains(&v) { Some(v) } else { None }
                })
            }
        }
    }

    pub fn to_string_with_prefix(&self, prefix: &str) -> String {
        match self {
            Self::Any => "*".to_string(),
            Self::Empty => "".to_string(),
            Self::Exact(v) => v.to_string(),
            Self::Range(r) => r.to_string(),
            Self::Union(sets) => {
                sets.iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" || ")
            }
            Self::Intersection(sets) => {
                sets.iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            Self::Complement(s) => format!("!({})", s.to_string()),
        }
    }
}

impl Default for VersionSet {
    fn default() -> Self {
        Self::Any
    }
}

impl fmt::Display for VersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_with_prefix(""))
    }
}

impl From<VersionRange> for VersionSet {
    fn from(r: VersionRange) -> Self {
        Self::Range(r)
    }
}

impl From<Version> for VersionSet {
    fn from(v: Version) -> Self {
        Self::Exact(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgpm_core::Version;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn r(s: &str) -> VersionRange {
        VersionRange::parse(s).unwrap()
    }

    #[test]
    fn test_contains() {
        let set = VersionSet::range(r("^1.0.0"));
        assert!(set.contains(&v("1.0.0")));
        assert!(set.contains(&v("1.5.0")));
        assert!(!set.contains(&v("0.9.0")));
        assert!(!set.contains(&v("2.0.0")));
    }

    #[test]
    fn test_union() {
        let a = VersionSet::range(r("^1.0.0"));
        let b = VersionSet::range(r("^2.0.0"));
        let union = a.union(&b);
        
        assert!(union.contains(&v("1.5.0")));
        assert!(union.contains(&v("2.5.0")));
        assert!(!union.contains(&v("0.9.0")));
    }

    #[test]
    fn test_intersection() {
        let a = VersionSet::range(r(">=1.0.0"));
        let b = VersionSet::range(r("<2.0.0"));
        let inter = a.intersection(&b);
        
        assert!(inter.contains(&v("1.5.0")));
        assert!(!inter.contains(&v("0.9.0")));
        assert!(!inter.contains(&v("2.0.0")));
    }

    #[test]
    fn test_complement() {
        let set = VersionSet::range(r("^1.0.0"));
        let comp = set.complement();
        
        assert!(!comp.contains(&v("1.0.0")));
        assert!(comp.contains(&v("0.9.0")));
        assert!(comp.contains(&v("2.0.0")));
    }
}
