use crate::error::MgError;
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: None,
        }
    }

    pub fn parse(input: &str) -> Result<Self, MgError> {
        let trimmed = input.trim().trim_start_matches('v');
        let (base, pre) = match trimmed.split_once('-') {
            Some((base, pre)) => (base, Some(pre.to_string())),
            None => (trimmed, None),
        };
        let mut parts = base.split('.');
        let major = parts
            .next()
            .ok_or_else(|| MgError::InvalidVersion(input.to_string()))?
            .parse()
            .map_err(|_| MgError::InvalidVersion(input.to_string()))?;
        let minor = parts
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|_| MgError::InvalidVersion(input.to_string()))?;
        let patch = parts
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|_| MgError::InvalidVersion(input.to_string()))?;
        Ok(Self {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.pre {
            Some(pre) => write!(f, "{}.{}.{}-{}", self.major, self.minor, self.patch, pre),
            None => write!(f, "{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            Ordering::Equal => compare_prerelease(self.pre.as_deref(), other.pre.as_deref()),
            ordering => ordering,
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => {
            let mut left_parts = left.split('.');
            let mut right_parts = right.split('.');

            loop {
                match (left_parts.next(), right_parts.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(left), Some(right)) => {
                        let left_num = left.parse::<u64>();
                        let right_num = right.parse::<u64>();
                        let ordering = match (left_num, right_num) {
                            (Ok(left), Ok(right)) => left.cmp(&right),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => left.cmp(right),
                        };
                        if ordering != Ordering::Equal {
                            return ordering;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Version;
    use std::cmp::Ordering;

    fn v(input: &str) -> Version {
        Version::parse(input).unwrap()
    }

    // --- Parse ---

    #[test]
    fn parse_v_prefix_stripped() {
        assert_eq!(v("v1.2.3"), v("1.2.3"));
    }

    #[test]
    fn parse_missing_minor_defaults_zero() {
        let v = v("1");
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
        assert!(v.pre.is_none());
    }

    #[test]
    fn parse_missing_patch_defaults_zero() {
        let v = v("1.2");
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn parse_with_prerelease() {
        let v = v("1.2.3-rc.1");
        assert_eq!(v.pre.as_deref(), Some("rc.1"));
    }

    #[test]
    fn parse_empty_returns_err() {
        assert!(Version::parse("").is_err());
    }

    #[test]
    fn parse_invalid_chars_returns_err() {
        assert!(Version::parse("1.x").is_err());
        assert!(Version::parse("a.b.c").is_err());
    }

    #[test]
    fn parse_whitespace_trimmed() {
        assert_eq!(v("  1.2.3  "), v("1.2.3"));
    }

    #[test]
    fn parse_v_only_returns_err() {
        assert!(Version::parse("v").is_err());
    }

    #[test]
    fn display_no_pre() {
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
    }

    #[test]
    fn display_with_pre() {
        assert_eq!(v("1.2.3-alpha").to_string(), "1.2.3-alpha");
    }

    // --- Cmp ---

    #[test]
    fn equal_versions_are_equal() {
        assert_eq!(v("1.0.0"), v("1.0.0"));
        assert_eq!(v("1.0.0").cmp(&v("1.0.0")), Ordering::Equal);
    }

    #[test]
    fn stable_greater_than_prerelease() {
        let stable = v("1.0.0");
        let prerelease = v("1.0.0-next.24");
        assert!(stable > prerelease);
    }

    #[test]
    fn numeric_prerelease_segments_sort_numerically() {
        let earlier = v("1.0.0-next.9");
        let later = v("1.0.0-next.24");
        assert!(later > earlier);
    }

    #[test]
    fn string_prerelease_segments_sort_lexically() {
        let a = v("1.0.0-alpha");
        let b = v("1.0.0-beta");
        assert!(a < b);
    }

    #[test]
    fn numeric_prerelease_less_than_string_prerelease() {
        // numeric fields compare less than string fields per semver spec
        let num = v("1.0.0-1");
        let string = v("1.0.0-alpha");
        assert!(num < string);
    }

    #[test]
    fn prerelease_with_more_segments_greater() {
        let short = v("1.0.0-alpha");
        let long = v("1.0.0-alpha.1");
        assert!(short < long);
    }

    #[test]
    fn major_version_dominates() {
        assert!(v("2.0.0") > v("1.9.9"));
    }

    #[test]
    fn minor_version_dominates() {
        assert!(v("1.2.0") > v("1.1.9"));
    }

    #[test]
    fn no_pre_equals_no_pre() {
        assert_eq!(v("0.0.0"), v("0.0.0"));
    }

    #[test]
    fn both_pre_identical_are_equal() {
        assert_eq!(v("1.0.0-rc1"), v("1.0.0-rc1"));
    }
}
