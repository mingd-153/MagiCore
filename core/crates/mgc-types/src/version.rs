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
