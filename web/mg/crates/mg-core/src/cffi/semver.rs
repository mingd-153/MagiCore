use std::ffi::CStr;
use std::ffi::CString;
use std::cmp::Ordering;

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

/// Parse a version string using C implementation.
/// Falls back to Rust parser if C returns error.
pub fn parse_version(s: &str) -> Result<Version, SemVerError> {
    let c_str = match CString::new(s) {
        Ok(s) => s,
        Err(_) => {
            // Fallback to Rust parser for strings with interior NUL
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
        return Version::parse(s); // fallback
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
        return a.cmp(b); // fallback
    }

    match unsafe { mg_version_cmp(&c_a, &c_b) } {
        -1 => Ordering::Less,
        0 => Ordering::Equal,
        1 => Ordering::Greater,
        _ => a.cmp(b), // fallback
    }
}

/// Check if a range contains a version using C implementation.
/// Returns None if parsing fails (caller should use Rust fallback).
///
/// This is the hot path function called many times during resolution.
pub fn range_contains(range_str: &str, version: &Version) -> Option<bool> {
    let range_cstr = CString::new(range_str).ok()?;
    let ver_str = version.to_string();
    let ver_cstr = CString::new(ver_str.as_str()).ok()?;

    let mut c_range = mg_range_t {
        type_: mg_range_type_t::MgRangeInvalid,
        min: mg_version_t { major: 0, minor: 0, patch: 0, prerelease: [0; 64], prerelease_len: -1 },
        max: mg_version_t { major: 0, minor: 0, patch: 0, prerelease: [0; 64], prerelease_len: -1 },
        sub_left: std::ptr::null_mut(),
        sub_right: std::ptr::null_mut(),
    };
    let mut c_ver = mg_version_t {
        major: 0, minor: 0, patch: 0,
        prerelease: [0; 64], prerelease_len: -1,
    };

    let range_ok = unsafe { mg_range_parse(range_cstr.as_ptr(), &mut c_range) == 0 };
    let ver_ok = unsafe { mg_version_parse(ver_cstr.as_ptr(), &mut c_ver) == 0 };

    if !range_ok || !ver_ok {
        return None;
    }

    Some(unsafe { mg_range_contains(&c_range, &c_ver) })
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
        // Verify C and Rust implementations agree for random cases
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
                "C mismatch for range='{}' version='{}': got {:?}", range, ver, c_result);

            // Only compare Rust for ranges it supports (not space-separated AND)
            if !range.contains(" <") {
                let rust_range = VersionRange::parse(range).unwrap();
                let rust_result = rust_range.contains(&v);
                assert_eq!(rust_result, expected,
                    "Rust mismatch for range='{}' version='{}'", range, ver);
            }
        }
    }
}
