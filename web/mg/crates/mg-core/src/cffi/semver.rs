use std::ffi::CStr;
use std::ffi::CString;
use std::cmp::Ordering;
use std::sync::LazyLock;

use dashmap::DashMap;

use crate::semver::Version;
use crate::SemVerError;

/* C API declarations */
#[allow(dead_code)]
extern "C" {
    fn mg_version_parse(s: *const std::os::raw::c_char, v: *mut mg_version_t) -> std::os::raw::c_int;
    fn mg_version_cmp(a: *const mg_version_t, b: *const mg_version_t) -> std::os::raw::c_int;
    fn mg_version_format(v: *const mg_version_t, out: *mut std::os::raw::c_char, out_len: usize) -> std::os::raw::c_int;
    fn mg_range_parse(s: *const std::os::raw::c_char, r: *mut mg_range_t) -> std::os::raw::c_int;
    fn mg_range_contains(r: *const mg_range_t, v: *const mg_version_t) -> bool;
}

#[repr(C)]
struct mg_version_t {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: [std::os::raw::c_char; 64],
    prerelease_len: std::os::raw::c_int,
}

#[repr(C)]
struct mg_range_t {
    type_: mg_range_type_t,
    min: mg_version_t,
    max: mg_version_t,
    sub_left: *mut mg_range_t,
    sub_right: *mut mg_range_t,
}

#[repr(C)]
#[allow(dead_code, clippy::enum_variant_names)]
enum mg_range_type_t {
    MgRangeExact,
    MgRangeCaret,
    MgRangeTilde,
    MgRangeGte,
    MgRangeGt,
    MgRangeLte,
    MgRangeLt,
    MgRangeStar,
    MgRangeAnd,
    MgRangeOr,
    MgRangeInvalid,
}

// ── ParsedRange cache ──────────────────────────────────────────────
// Caches parsed range representations keyed by range string.
// Eliminates ~780k CString + C mg_range_parse calls per install.

static RANGE_CACHE: LazyLock<DashMap<String, CachedRange>> = LazyLock::new(DashMap::new);

#[derive(Clone)]
enum CachedRange {
    Parsed(ParsedRange),
    Unparseable,
}

#[derive(Clone)]
enum ParsedRange {
    All,
    Exact(Version),
    Caret(Version, Version),
    Tilde(Version, Version),
    Gte(Version),
    Gt(Version),
    Lte(Version),
    Lt(Version),
    Or(Vec<ParsedRange>),
    And(Vec<ParsedRange>),
}

impl ParsedRange {
    fn contains(&self, version: &Version) -> bool {
        match self {
            ParsedRange::All => true,
            ParsedRange::Exact(v) => version == v,
            ParsedRange::Caret(min, max) => {
                if version.prerelease.is_some() {
                    let base = Version::new(version.major, version.minor, version.patch);
                    if !(base >= *min && base < *max) {
                        return false;
                    }
                }
                *version >= *min && *version < *max
            }
            ParsedRange::Tilde(min, max) => {
                if version.prerelease.is_some() {
                    let base = Version::new(version.major, version.minor, version.patch);
                    if !(base >= *min && base < *max) {
                        return false;
                    }
                }
                *version >= *min && *version < *max
            }
            ParsedRange::Gte(v) => *version >= *v,
            ParsedRange::Gt(v) => *version > *v,
            ParsedRange::Lte(v) => *version <= *v,
            ParsedRange::Lt(v) => *version < *v,
            ParsedRange::Or(ranges) => ranges.iter().any(|r| r.contains(version)),
            ParsedRange::And(ranges) => ranges.iter().all(|r| r.contains(version)),
        }
    }
}

fn increment_major(v: &Version) -> Version {
    Version {
        major: v.major + 1,
        minor: 0,
        patch: 0,
        prerelease: None,
        build: None,
    }
}

fn increment_minor(v: &Version) -> Version {
    Version {
        major: v.major,
        minor: v.minor + 1,
        patch: 0,
        prerelease: None,
        build: None,
    }
}

fn parse_single_range(s: &str) -> Option<ParsedRange> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if s == "*" {
        return Some(ParsedRange::All);
    }

    if let Some(min) = s.strip_prefix('^') {
        if let Ok(v) = Version::parse(min.trim()) {
            let max = increment_major(&v);
            return Some(ParsedRange::Caret(v, max));
        }
    }

    if let Some(min) = s.strip_prefix('~') {
        if let Ok(v) = Version::parse(min.trim()) {
            let max = increment_minor(&v);
            return Some(ParsedRange::Tilde(v, max));
        }
    }

    if let Some(min) = s.strip_prefix(">=") {
        if let Ok(v) = Version::parse(min.trim()) {
            return Some(ParsedRange::Gte(v));
        }
    }

    if let Some(min) = s.strip_prefix('>') {
        if let Ok(v) = Version::parse(min.trim()) {
            return Some(ParsedRange::Gt(v));
        }
    }

    if let Some(max) = s.strip_prefix("<=") {
        if let Ok(v) = Version::parse(max.trim()) {
            return Some(ParsedRange::Lte(v));
        }
    }

    if let Some(max) = s.strip_prefix('<') {
        if let Ok(v) = Version::parse(max.trim()) {
            return Some(ParsedRange::Lt(v));
        }
    }

    if let Ok(v) = Version::parse(s) {
        return Some(ParsedRange::Exact(v));
    }

    None
}

fn parse_range(s: &str) -> Option<ParsedRange> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(or_ranges) = parse_or_ranges(s) {
        return Some(ParsedRange::Or(or_ranges));
    }

    if let Some(and_ranges) = parse_and_ranges(s) {
        return Some(ParsedRange::And(and_ranges));
    }

    parse_single_range(s)
}

fn parse_or_ranges(s: &str) -> Option<Vec<ParsedRange>> {
    let mut parts = Vec::new();
    for part in s.split("||") {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        if let Some(and_ranges) = parse_and_ranges(part) {
            if and_ranges.len() == 1 {
                parts.push(and_ranges.into_iter().next().unwrap());
            } else {
                parts.push(ParsedRange::And(and_ranges));
            }
        } else if let Some(single) = parse_single_range(part) {
            parts.push(single);
        } else {
            return None;
        }
    }
    if parts.len() >= 2 {
        Some(parts)
    } else {
        None
    }
}

fn parse_and_ranges(s: &str) -> Option<Vec<ParsedRange>> {
    let mut parts = Vec::new();
    for part in s.split_whitespace() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(single) = parse_single_range(part) {
            parts.push(single);
        } else {
            return None;
        }
    }
    if parts.len() >= 2 {
        Some(parts)
    } else {
        None
    }
}

/// Check if a range contains a version.
///
/// Uses a parsed-range cache to avoid re-parsing range strings.
/// Falls back to C FFI for correctness verification in tests.
pub fn range_contains(range_str: &str, version: &Version) -> Option<bool> {
    if let Some(entry) = RANGE_CACHE.get(range_str) {
        return match &*entry {
            CachedRange::Parsed(p) => Some(p.contains(version)),
            CachedRange::Unparseable => None,
        };
    }

    let parsed = match parse_range(range_str) {
        Some(p) => {
            RANGE_CACHE.insert(range_str.to_string(), CachedRange::Parsed(p.clone()));
            p
        }
        None => {
            RANGE_CACHE.insert(range_str.to_string(), CachedRange::Unparseable);
            return None;
        }
    };

    Some(parsed.contains(version))
}

/// Parse a version string using C implementation.
/// Falls back to Rust parser if C returns error.
pub fn parse_version(s: &str) -> Result<Version, SemVerError> {
    let c_str = match CString::new(s) {
        Ok(s) => s,
        Err(_) => {
            return Version::parse(s);
        }
    };

    let mut c_ver = mg_version_t {
        major: 0,
        minor: 0,
        patch: 0,
        prerelease: [0; 64],
        prerelease_len: -1,
    };

    let ret = unsafe { mg_version_parse(c_str.as_ptr(), &mut c_ver) };
    if ret != 0 {
        return Version::parse(s);
    }

    let prerelease = if c_ver.prerelease_len >= 0 {
        let s = unsafe { CStr::from_ptr(c_ver.prerelease.as_ptr()) };
        Some(s.to_string_lossy().into_owned())
    } else {
        None
    };

    Ok(Version {
        major: c_ver.major,
        minor: c_ver.minor,
        patch: c_ver.patch,
        prerelease,
        build: None,
    })
}

/// Compare two versions using C implementation.
pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let a_fmt = a.to_string();
    let b_fmt = b.to_string();

    let a_cstr = CString::new(a_fmt.as_str()).unwrap_or_default();
    let b_cstr = CString::new(b_fmt.as_str()).unwrap_or_default();

    let mut c_a = mg_version_t {
        major: 0, minor: 0, patch: 0,
        prerelease: [0; 64], prerelease_len: -1,
    };
    let mut c_b = mg_version_t {
        major: 0, minor: 0, patch: 0,
        prerelease: [0; 64], prerelease_len: -1,
    };

    let ok_a = unsafe { mg_version_parse(a_cstr.as_ptr(), &mut c_a) == 0 };
    let ok_b = unsafe { mg_version_parse(b_cstr.as_ptr(), &mut c_b) == 0 };

    if !ok_a || !ok_b {
        return a.cmp(b);
    }

    match unsafe { mg_version_cmp(&c_a, &c_b) } {
        -1 => Ordering::Less,
        0 => Ordering::Equal,
        1 => Ordering::Greater,
        _ => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VersionRange;

    #[test]
    fn test_c_version_parse() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_c_version_parse_prerelease() {
        let v = parse_version("1.0.0-next.9").unwrap();
        assert_eq!(v.prerelease.as_deref(), Some("next.9"));
    }

    #[test]
    fn test_c_compare_versions() {
        let a = Version::parse("2.0.0").unwrap();
        let b = Version::parse("1.0.0").unwrap();
        assert_eq!(compare_versions(&a, &b), Ordering::Greater);
        assert_eq!(compare_versions(&b, &a), Ordering::Less);
    }

    #[test]
    fn test_c_prerelease_numeric_ordering() {
        let v9 = Version::parse("1.0.0-next.9").unwrap();
        let v24 = Version::parse("1.0.0-next.24").unwrap();
        assert_eq!(compare_versions(&v9, &v24), Ordering::Less);
    }

    #[test]
    fn test_c_range_contains() {
        let result = range_contains("^1.0.0", &Version::parse("1.5.0").unwrap());
        assert_eq!(result, Some(true));

        let result = range_contains("^1.0.0", &Version::parse("2.0.0").unwrap());
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_c_range_contains_polka_regression() {
        let v9 = Version::parse("1.0.0-next.9").unwrap();
        let v24 = Version::parse("1.0.0-next.24").unwrap();
        let v29 = Version::parse("1.0.0-next.29").unwrap();

        assert_eq!(range_contains("^1.0.0-next.24", &v9), Some(false));
        assert_eq!(range_contains("^1.0.0-next.24", &v24), Some(true));
        assert_eq!(range_contains("^1.0.0-next.24", &v29), Some(true));
    }

    #[test]
    fn test_c_range_fallback_same_as_rust() {
        let cases = vec![
            ("^1.0.0", "1.0.0", true),
            ("^1.0.0", "1.0.1", true),
            ("^1.0.0", "2.0.0", false),
            ("^1.0.0", "0.9.0", false),
            ("~1.2.0", "1.2.0", true),
            ("~1.2.0", "1.2.9", true),
            ("~1.2.0", "1.3.0", false),
            ("*", "1.0.0", true),
            (">=1.0.0", "1.0.0", true),
            (">=1.0.0", "2.0.0", true),
            (">=1.0.0", "0.9.9", false),
            ("1.2.3", "1.2.3", true),
            ("1.2.3", "1.2.4", false),
            (">=1.0.0 <2.0.0", "1.5.0", true),
            (">=1.0.0 <2.0.0", "2.0.0", false),
            ("^1.0.0 || ^2.0.0", "1.5.0", true),
            ("^1.0.0 || ^2.0.0", "2.5.0", true),
            ("^1.0.0 || ^2.0.0", "3.0.0", false),
        ];

        for (range, ver, expected) in cases {
            let v = Version::parse(ver).unwrap();
            let c_result = range_contains(range, &v);
            assert_eq!(c_result, Some(expected),
                "Cached range mismatch for range='{}' version='{}': got {:?}", range, ver, c_result);

            if !range.contains(" <") {
                let rust_range = VersionRange::parse(range).unwrap();
                let rust_result = rust_range.contains(&v);
                assert_eq!(rust_result, expected,
                    "Rust mismatch for range='{}' version='{}'", range, ver);
            }
        }
    }

    #[test]
    fn test_range_cache_hit() {
        let range = "^1.0.0";
        let v1 = Version::parse("1.5.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert_eq!(range_contains(range, &v1), Some(true));
        assert_eq!(range_contains(range, &v2), Some(false));
        assert!(RANGE_CACHE.contains_key(range));
    }

    #[test]
    fn test_parse_range_star() {
        let p = parse_range("*").unwrap();
        assert!(p.contains(&Version::new(0, 0, 0)));
        assert!(p.contains(&Version::new(999, 999, 999)));
    }

    #[test]
    fn test_parse_range_caret() {
        let p = parse_range("^1.2.3").unwrap();
        assert!(p.contains(&Version::new(1, 2, 3)));
        assert!(p.contains(&Version::new(1, 9, 99)));
        assert!(!p.contains(&Version::new(2, 0, 0)));
        assert!(!p.contains(&Version::new(0, 9, 0)));
    }

    #[test]
    fn test_parse_range_tilde() {
        let p = parse_range("~1.2.0").unwrap();
        assert!(p.contains(&Version::new(1, 2, 0)));
        assert!(p.contains(&Version::new(1, 2, 9)));
        assert!(!p.contains(&Version::new(1, 3, 0)));
    }

    #[test]
    fn test_parse_range_gte() {
        let p = parse_range(">=1.0.0").unwrap();
        assert!(p.contains(&Version::new(1, 0, 0)));
        assert!(p.contains(&Version::new(2, 0, 0)));
        assert!(!p.contains(&Version::new(0, 9, 9)));
    }

    #[test]
    fn test_parse_range_and() {
        let p = parse_range(">=1.0.0 <2.0.0").unwrap();
        assert!(p.contains(&Version::new(1, 5, 0)));
        assert!(!p.contains(&Version::new(2, 0, 0)));
        assert!(!p.contains(&Version::new(0, 9, 0)));
    }

    #[test]
    fn test_parse_range_or() {
        let p = parse_range("^1.0.0 || ^2.0.0").unwrap();
        assert!(p.contains(&Version::new(1, 5, 0)));
        assert!(p.contains(&Version::new(2, 5, 0)));
        assert!(!p.contains(&Version::new(3, 0, 0)));
    }

    #[test]
    fn test_parse_unparseable() {
        assert!(parse_range("").is_none());
        assert!(parse_range("not-a-range").is_none());
    }
}
