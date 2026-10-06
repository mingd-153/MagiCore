#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_types::version::Version;
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

// --- PEP 440 post/dev segments (no silent truncation) ---

#[test]
fn parse_post_release_keeps_post_number() {
    let v = v("1.2.3.post1");
    assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
    assert_eq!(v.post, Some(1));
    assert_eq!(v.dev, None);
    assert_eq!(v.to_string(), "1.2.3.post1");
}

#[test]
fn parse_dev_release_keeps_dev_number() {
    let v = v("2.0.dev0");
    assert_eq!(v.dev, Some(0));
    assert_eq!(v.post, None);
    assert_eq!(v.to_string(), "2.0.0.dev0");
}

#[test]
fn parse_bare_post_defaults_to_zero() {
    assert_eq!(v("1.2.3.post").post, Some(0));
}

#[test]
fn parse_extra_segments_rejects_loudly() {
    // Previously "1.2.3.post1" silently became "1.2.3" — truncation,
    // not leniency. Unknown shapes must fail closed.
    assert!(Version::parse("1.2.3.4").is_err());
    assert!(Version::parse("1.2.3.foo").is_err());
    assert!(Version::parse("1.2.3.postx").is_err());
    assert!(Version::parse("1.2.3.post1.2").is_err());
}

#[test]
fn ordering_dev_below_pre_below_final_below_post() {
    use Ordering::*;
    assert_eq!(v("1.2.3.dev1").cmp(&v("1.2.3")), Less, "dev < final");
    assert_eq!(
        v("1.2.3.dev1").cmp(&v("1.2.3-rc.1")),
        Less,
        "dev < prerelease"
    );
    assert_eq!(v("1.2.3-rc.1").cmp(&v("1.2.3")), Less, "prerelease < final");
    assert_eq!(v("1.2.3").cmp(&v("1.2.3.post1")), Less, "final < post");
    assert_eq!(
        v("1.2.3.post1").cmp(&v("1.2.3.post2")),
        Less,
        "post orders numerically"
    );
    assert_eq!(v("1.2.3.post1").cmp(&v("1.2.3.post1")), Equal);
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
