#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_types::error::MgError;
use mgc_types::package::*;
use mgc_types::version::Version;

fn v(input: &str) -> Version {
    Version::parse(input).unwrap()
}

fn pn(input: &str) -> PackageName {
    PackageName::new(input).unwrap()
}


// --- PackageName::new() ---

#[test]
fn package_name_valid_simple() {
    let name = pn("lodash");
    assert_eq!(name.as_str(), "lodash");
}

#[test]
fn package_name_valid_scoped() {
    let name = pn("@scope/package");
    assert_eq!(name.as_str(), "@scope/package");
}

#[test]
fn package_name_valid_with_chars() {
    let name = pn("my-awesome_package!1");
    assert_eq!(name.as_str(), "my-awesome_package!1");
}

#[test]
fn package_name_empty_rejected() {
    assert!(matches!(
        PackageName::new(""),
        Err(MgError::InvalidPackageName(_))
    ));
}

#[test]
fn package_name_whitespace_only_rejected() {
    assert!(PackageName::new("   ").is_err());
}

#[test]
fn package_name_too_long_rejected() {
    let long = "a".repeat(215);
    assert!(PackageName::new(long).is_err());
}

#[test]
fn package_name_null_byte_rejected() {
    assert!(PackageName::new("foo\0bar").is_err());
}

#[test]
fn package_name_newline_rejected() {
    assert!(PackageName::new("foo\nbar").is_err());
}

#[test]
fn package_name_carriage_return_rejected() {
    assert!(PackageName::new("foo\rbar").is_err());
}

#[test]
fn package_name_path_traversal_rejected() {
    assert!(PackageName::new("..").is_err());
    assert!(PackageName::new("foo/..").is_err());
}

#[test]
fn package_name_starts_with_slash_rejected() {
    assert!(PackageName::new("/foo").is_err());
}

#[test]
fn package_name_starts_with_dot_rejected() {
    assert!(PackageName::new(".hidden").is_err());
}

#[test]
fn package_name_backslash_rejected() {
    assert!(PackageName::new("foo\\bar").is_err());
}

#[test]
fn package_name_tilde_rejected() {
    // tilde fails before ascii check via the ~ check on line 26
    assert!(PackageName::new("foo~bar").is_err());
}

#[test]
fn package_name_scoped_empty_org_rejected() {
    assert!(PackageName::new("@/pkg").is_err());
}

#[test]
fn package_name_scoped_empty_pkg_rejected() {
    assert!(PackageName::new("@org/").is_err());
}

#[test]
fn package_name_multiple_slashes_rejected() {
    assert!(PackageName::new("a/b/c").is_err());
}

#[test]
fn package_name_non_ascii_rejected() {
    assert!(PackageName::new("café").is_err());
}

#[test]
fn package_name_unscoped_returns_self() {
    assert_eq!(pn("lodash").unscoped(), "lodash");
}

#[test]
fn package_name_unscoped_strips_scope() {
    assert_eq!(pn("@scope/package").unscoped(), "package");
}

// --- VersionRange::matches() ---

#[test]
fn range_star_matches_anything() {
    let range = VersionRange::star();
    assert!(range.matches(&v("0.0.0")));
    assert!(range.matches(&v("999.999.999")));
}

#[test]
fn range_tilde_matches_patch_within_minor() {
    let range = VersionRange::parse("~1.2.3").unwrap();
    assert!(range.matches(&v("1.2.4")));
    assert!(!range.matches(&v("1.3.0")));
}

#[test]
fn range_gte_matches() {
    let range = VersionRange::parse(">=2.0.0").unwrap();
    assert!(range.matches(&v("2.0.0")));
    assert!(range.matches(&v("3.0.0")));
    assert!(!range.matches(&v("1.9.9")));
}

#[test]
fn range_lte_matches() {
    let range = VersionRange::parse("<=2.0.0").unwrap();
    assert!(range.matches(&v("2.0.0")));
    assert!(range.matches(&v("1.0.0")));
    assert!(!range.matches(&v("3.0.0")));
}

#[test]
fn range_gt_matches() {
    let range = VersionRange::parse(">2.0.0").unwrap();
    assert!(range.matches(&v("2.0.1")));
    assert!(!range.matches(&v("2.0.0")));
}

#[test]
fn range_lt_matches() {
    let range = VersionRange::parse("<2.0.0").unwrap();
    assert!(range.matches(&v("1.9.9")));
    assert!(!range.matches(&v("2.0.0")));
}

#[test]
fn range_or_with_three_alternatives() {
    let range = VersionRange::parse("1.0.0 || 2.0.0 || 3.0.0").unwrap();
    assert!(range.matches(&v("1.0.0")));
    assert!(range.matches(&v("2.0.0")));
    assert!(range.matches(&v("3.0.0")));
    assert!(!range.matches(&v("1.5.0")));
}

#[test]
fn range_compound_mixed_operators() {
    let range = VersionRange::parse(">=1.0.0 <2.0.0 || >=3.0.0").unwrap();
    assert!(range.matches(&v("1.5.0")));
    assert!(!range.matches(&v("2.5.0")));
    assert!(range.matches(&v("3.0.0")));
}

#[test]
fn range_parse_empty_returns_err() {
    assert!(VersionRange::parse("").is_err());
}

#[test]
fn range_is_star() {
    assert!(VersionRange::star().is_star());
    assert!(!VersionRange::parse("1.0.0").unwrap().is_star());
}

// --- VersionRange::satisfying_version() ---

#[test]
fn satisfying_version_star_returns_000() {
    assert_eq!(VersionRange::star().satisfying_version(), Some(v("0.0.0")));
}

#[test]
fn satisfying_version_caret() {
    let range = VersionRange::parse("^1.2.3").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("1.2.3")));
}

#[test]
fn satisfying_version_tilde() {
    let range = VersionRange::parse("~1.2.3").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("1.2.3")));
}

#[test]
fn satisfying_version_gt() {
    let range = VersionRange::parse(">1.2.3").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("1.2.3")));
}

#[test]
fn satisfying_version_lt() {
    let range = VersionRange::parse("<2.0.0").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("2.0.0")));
}

#[test]
fn satisfying_version_gte() {
    let range = VersionRange::parse(">=1.2.3").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("1.2.3")));
}

#[test]
fn satisfying_version_or_takes_first() {
    let range = VersionRange::parse("1.0.0 || 2.0.0").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("1.0.0")));
}

// --- PackageId::parse() ---

#[test]
fn package_id_parse_valid() {
    let id = PackageId::parse("lodash@1.2.3").unwrap();
    assert_eq!(id.name_str(), "lodash");
    assert_eq!(id.version(), &v("1.2.3"));
}

#[test]
fn package_id_parse_missing_at_returns_err() {
    assert!(PackageId::parse("lodash").is_err());
}

#[test]
fn package_id_parse_multiple_at_uses_last() {
    // rfind('@') picks last @: name="a@b", version="1.2.3"
    let id = PackageId::parse("a@b@1.2.3").unwrap();
    assert_eq!(id.name_str(), "a@b");
    assert_eq!(id.version(), &v("1.2.3"));
}

#[test]
fn package_id_parse_empty_name_returns_err() {
    assert!(PackageId::parse("@1.2.3").is_err());
}

#[test]
fn package_id_display() {
    let id = PackageId::parse("lodash@1.2.3").unwrap();
    assert_eq!(id.to_string(), "lodash@1.2.3");
}

// --- DependencySpec::parse() ---

#[test]
fn dep_spec_without_at_uses_star() {
    let spec = DependencySpec::parse("lodash").unwrap();
    assert_eq!(spec.name.as_str(), "lodash");
    assert!(spec.range.is_star());
}

#[test]
fn dep_spec_with_at_and_version() {
    let spec = DependencySpec::parse("lodash@^1.2.0").unwrap();
    assert_eq!(spec.name.as_str(), "lodash");
    assert!(!spec.range.is_star());
}

#[test]
fn dep_spec_with_at_empty_version_returns_err() {
    // VersionRange::parse("") fails so @ with nothing after is an error
    assert!(DependencySpec::parse("lodash@").is_err());
}

#[test]
fn dep_spec_new_defaults_false() {
    let name = pn("foo");
    let range = VersionRange::star();
    let spec = DependencySpec::new(name, range);
    assert!(!spec.dev);
    assert!(!spec.optional);
    assert!(!spec.peer);
}

// --- VersionRange Display ---

#[test]
fn range_display() {
    let range = VersionRange::parse(">=1.0.0").unwrap();
    assert_eq!(format!("{range}"), ">=1.0.0");
}

// --- Existing tests preserved ---

#[test]
fn exact_equals_range_matches_same_version() {
    let range = VersionRange::parse("=0.139.0").unwrap();
    assert!(range.matches(&v("0.139.0")));
    assert!(!range.matches(&v("0.139.1")));
}

#[test]
fn exact_equals_range_produces_satisfying_version() {
    let range = VersionRange::parse("=18.3.1").unwrap();
    assert_eq!(range.satisfying_version(), Some(v("18.3.1")));
}

#[test]
fn compound_range_matches_intersection() {
    let range = VersionRange::parse(">=0.2.0 <0.5.0").unwrap();
    assert!(range.matches(&v("0.4.9")));
    assert!(!range.matches(&v("0.5.0")));
    assert!(!range.matches(&v("0.1.9")));
}

#[test]
fn compound_range_matches_spaced_comparators() {
    let range = VersionRange::parse(">= 2.1.2 < 3.0.0").unwrap();
    assert!(range.matches(&v("2.8.1")));
    assert!(!range.matches(&v("2.1.1")));
    assert!(!range.matches(&v("3.0.0")));
}

#[test]
fn npm_double_equals_matches_same_version() {
    let range = VersionRange::parse("==0.139.0").unwrap();
    assert!(range.matches(&v("0.139.0")));
    assert!(!range.matches(&v("0.139.1")));
}

#[test]
fn npm_double_equals_resolves_as_op() {
    let range = VersionRange::parse("== 0.139.0").unwrap();
    assert!(range.matches(&v("0.139.0")));
    assert!(!range.matches(&v("0.139.1")));
}

#[test]
fn npm_single_equals_matches_oxc_types() {
    let range = VersionRange::parse("=0.139.0").unwrap();
    assert!(range.matches(&v("0.139.0")));
    assert!(!range.matches(&v("0.138.0")));
    assert!(!range.matches(&v("0.139.1")));
}

#[test]
fn caret_range_matches_within_major() {
    let range = VersionRange::parse("^1.2.3").unwrap();
    assert!(range.matches(&v("1.5.0")));
    assert!(!range.matches(&v("2.0.0")));
}

// --- G1 Fix: Wildcard ranges with comparison operators ---

#[test]
fn wildcard_range_gte_matches() {
    let range = VersionRange::parse(">=22.x").unwrap();
    assert!(range.matches(&v("22.0.0")));
    assert!(range.matches(&v("22.5.0")));
    assert!(range.matches(&v("23.0.0")));
    assert!(range.matches(&v("24.0.0")));
    assert!(!range.matches(&v("21.9.9")));
}

#[test]
fn wildcard_range_lte_matches() {
    let range = VersionRange::parse("<=24.x").unwrap();
    assert!(range.matches(&v("24.0.0")));
    assert!(range.matches(&v("23.5.0")));
    assert!(range.matches(&v("22.0.0")));
    assert!(!range.matches(&v("25.0.0")));
}

#[test]
fn wildcard_compound_range_gte_lte() {
    // This is the puppeteer-core pattern that was failing
    let range = VersionRange::parse(">=22.x <=24.x").unwrap();
    assert!(range.matches(&v("22.0.0")));
    assert!(range.matches(&v("23.5.0")));
    assert!(range.matches(&v("24.0.0")));
    assert!(!range.matches(&v("21.9.9")));
    assert!(!range.matches(&v("25.0.0")));
}
