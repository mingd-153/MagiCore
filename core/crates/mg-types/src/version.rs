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
        (self.major, self.minor, self.patch, self.pre.is_some(), &self.pre).cmp(&(
            other.major,
            other.minor,
            other.patch,
            other.pre.is_some(),
            &other.pre,
        ))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
