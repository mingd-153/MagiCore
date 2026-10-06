use crate::error::MgError;
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
    /// PEP 440 dev release (`1.2.3.dev1`) — sorts below every
    /// pre-release/final/post of the same triple. `None` = not a dev release.
    /// Bản dev PEP 440 xếp trước pre-release/final/post cùng bộ số; `None` = không phải dev.
    #[serde(default)]
    pub dev: Option<u64>,
    /// PEP 440 post release (`1.2.3.post1`) — sorts above the final release
    /// of the same triple. `None` = no post segment.
    /// Bản post PEP 440 xếp sau final cùng bộ số; `None` = không có hậu tố post.
    #[serde(default)]
    pub post: Option<u64>,
}

/// True for a PEP 440 post/dev segment shape (`post`, `postN`, `dev`,
/// `devN`) — digits-only suffix, empty means release 0.
/// (Đúng với shape post/dev PEP 440 — hậu tố chỉ số, rỗng nghĩa là 0.)
fn is_post_or_dev_segment(segment: &str) -> bool {
    let after = if let Some(after) = segment.strip_prefix("post") {
        after
    } else if let Some(after) = segment.strip_prefix("dev") {
        after
    } else {
        return false;
    };
    after.is_empty() || after.bytes().all(|b| b.is_ascii_digit())
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: None,
            dev: None,
            post: None,
        }
    }

    /// Parse `1.2.3`, `1.2`, `1`, `v`-prefixed, and `-pre` suffixes (all
    /// pre-existing leniencies, unchanged). Extra dot-segments are PEP 440
    /// post/dev releases (`1.2.3.post1`, `1.2.3.dev0`); anything else
    /// after the patch is an error instead of a silent truncation
    /// (previously `1.2.3.post1` became `1.2.3`).
    /// Chấp nhận hậu tố post/dev PEP 440; hậu tố khác hoặc nhiều segment sẽ báo lỗi thay vì bị cắt bỏ.
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
        // PEP 440 short form: "2.0.dev0" means "2.0.0.dev0" — a post/dev
        // segment may sit in the patch position when the patch is absent.
        // Dạng rút gọn: "2.0.dev0" tương đương "2.0.0.dev0" khi thiếu patch.
        let patch_raw = parts.next().unwrap_or("0");
        let (patch, short_extra): (u64, Option<&str>) = match patch_raw.parse() {
            Ok(patch) => (patch, None),
            Err(_) => {
                if is_post_or_dev_segment(patch_raw) {
                    (0, Some(patch_raw))
                } else {
                    return Err(MgError::InvalidVersion(input.to_string()));
                }
            }
        };
        let mut dev = None;
        let mut post = None;
        // A short-form segment from the patch position counts as the single
        // allowed extra segment — anything after it is not a version.
        // Hậu tố ở vị trí patch được tính là segment duy nhất; dữ liệu nối thêm sẽ bị từ chối.
        let short_rest: Vec<&str> = short_extra.into_iter().collect();
        let mut rest: Vec<&str> = short_rest;
        if let Some(extra) = parts.next() {
            rest.push(extra);
            rest.extend(parts);
        }
        if !rest.is_empty() {
            // This parser supports one post/dev segment (`postN`,
            // `devN`, bare `post`/`dev` = 0); more segments or any other
            // shape is not a version we understand — refuse loudly.
            // Chỉ hỗ trợ một segment post/dev; segment thừa hoặc sai dạng bị từ chối rõ ràng.
            if rest.len() != 1 {
                return Err(MgError::InvalidVersion(input.to_string()));
            }
            let segment = rest[0];
            let (is_post, digits) = if let Some(after) = segment.strip_prefix("post") {
                (true, after)
            } else if let Some(after) = segment.strip_prefix("dev") {
                (false, after)
            } else {
                return Err(MgError::InvalidVersion(input.to_string()));
            };
            let digits = if digits.is_empty() { "0" } else { digits };
            let number: u64 = digits
                .parse()
                .map_err(|_| MgError::InvalidVersion(input.to_string()))?;
            if is_post {
                post = Some(number);
            } else {
                dev = Some(number);
            }
        }
        Ok(Self {
            major,
            minor,
            patch,
            pre,
            dev,
            post,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        if let Some(dev) = self.dev {
            write!(f, ".dev{dev}")?;
        }
        if let Some(post) = self.post {
            write!(f, ".post{post}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            Ordering::Equal => match (self.dev, other.dev) {
                // PEP 440: dev releases sort below everything else.
                // Theo PEP 440, bản dev xếp trước mọi dạng phát hành còn lại.
                (Some(left), Some(right)) => match left.cmp(&right) {
                    Ordering::Equal => {
                        match compare_prerelease(self.pre.as_deref(), other.pre.as_deref()) {
                            Ordering::Equal => self.post.cmp(&other.post),
                            ordering => ordering,
                        }
                    }
                    ordering => ordering,
                },
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => match compare_prerelease(self.pre.as_deref(), other.pre.as_deref())
                {
                    Ordering::Equal => self.post.cmp(&other.post),
                    ordering => ordering,
                },
            },
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
