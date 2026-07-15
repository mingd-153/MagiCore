use mg_types::{Version, VersionRange};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum VersionSet {
    #[default]
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
            Self::Range(r) => r.matches(version),
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
            (Self::Empty, s) | (s, Self::Empty) => s.clone(),
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
                if r.matches(v) {
                    Self::Exact(v.clone())
                } else {
                    Self::Empty
                }
            }
            (Self::Range(_), Self::Range(_)) => {
                Self::Intersection(vec![self.clone(), other.clone()])
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
            Self::Any => Some(Version::new(0, 0, 0)),
            Self::Empty => None,
            Self::Exact(v) => Some(v.clone()),
            Self::Range(r) => r.satisfying_version(),
            Self::Union(sets) => sets.iter().filter_map(|s| s.satisfying_version()).max(),
            Self::Intersection(sets) => sets
                .iter()
                .filter_map(|s| s.satisfying_version())
                .find(|v| sets.iter().all(|s| s.contains(v))),
            Self::Complement(s) => s.satisfying_version().filter(|v| !self.contains(v)),
        }
    }
}

impl fmt::Display for VersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "*"),
            Self::Empty => write!(f, "<none>"),
            Self::Exact(v) => write!(f, "{v}"),
            Self::Range(r) => write!(f, "{r}"),
            Self::Union(sets) => {
                for (i, s) in sets.iter().enumerate() {
                    if i > 0 {
                        write!(f, " || ")?;
                    }
                    write!(f, "{s}")?;
                }
                Ok(())
            }
            Self::Intersection(sets) => {
                for s in sets {
                    write!(f, "{s} ")?;
                }
                Ok(())
            }
            Self::Complement(s) => write!(f, "!({s})"),
        }
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
    }

    #[test]
    fn test_union() {
        let a = VersionSet::range(r("^1.0.0"));
        let b = VersionSet::range(r("^2.0.0"));
        let u = a.union(&b);
        assert!(u.contains(&v("1.5.0")));
        assert!(u.contains(&v("2.5.0")));
    }

    #[test]
    fn test_intersection() {
        let a = VersionSet::range(r(">=1.0.0"));
        let b = VersionSet::range(r("<2.0.0"));
        let i = a.intersection(&b);
        assert!(i.contains(&v("1.5.0")));
        assert!(!i.contains(&v("2.0.0")));
    }

    #[test]
    fn test_complement() {
        let set = VersionSet::range(r("^1.0.0"));
        let comp = set.complement();
        assert!(comp.contains(&v("0.9.0")));
        assert!(comp.contains(&v("2.0.0")));
        assert!(!comp.contains(&v("1.5.0")));
    }
}
