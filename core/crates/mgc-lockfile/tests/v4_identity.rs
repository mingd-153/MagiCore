//! v4 identity tests: canonical naming, versions, peer digests, source
//! selection (design §§2/5). No registry, no filesystem — pure logic.
//! (Test identity v4: logic thuần, không mạng/không đĩa.)

use mgc_lockfile::EcosystemTag;
use mgc_lockfile::v4::{
    SourceRef, SourceSelectionError, TargetTuple, canonical_name, canonical_version, claim_matches,
    claim_specificity, peer_context_digest, select_source,
};

fn tag() -> EcosystemTag {
    EcosystemTag::Web
}

#[test]
fn canonical_name_npm_lowercases() {
    assert_eq!(canonical_name(tag(), "  React  "), "react");
    assert_eq!(canonical_name(tag(), "@Scope/Package"), "@scope/package");
}

#[test]
fn canonical_name_pypi_pep503() {
    let python = EcosystemTag::Python;
    assert_eq!(canonical_name(python, "Foo_Bar.Baz"), "foo-bar-baz");
    assert_eq!(canonical_name(python, "requests"), "requests");
    assert_eq!(canonical_name(python, "a--b__c..d"), "a-b-c-d");
}

#[test]
fn canonical_name_nuget_lowercases() {
    assert_eq!(
        canonical_name(EcosystemTag::NuGet, "Newtonsoft.Json"),
        "newtonsoft.json"
    );
}

#[test]
fn canonical_version_pads_short_forms() {
    assert_eq!(canonical_version("1"), "1.0.0");
    assert_eq!(canonical_version("1.2"), "1.2.0");
    assert_eq!(canonical_version("1.2.3"), "1.2.3");
    assert_eq!(canonical_version("2.0.0-beta.1"), "2.0.0-beta.1");
    assert_eq!(canonical_version("10.20.30+build.5"), "10.20.30+build.5");
}

#[test]
fn peer_context_digest_is_order_independent_and_stable() {
    let a = vec![
        ("react".to_string(), "18.2.0".to_string()),
        ("lodash".to_string(), "4.17.21".to_string()),
    ];
    let b = vec![
        ("lodash".to_string(), "4.17.21".to_string()),
        ("react".to_string(), "18.2.0".to_string()),
    ];
    assert_eq!(peer_context_digest(&a), peer_context_digest(&b));
    assert_eq!(peer_context_digest(&a).len(), 64);
    let c = vec![("react".to_string(), "18.3.0".to_string())];
    assert_ne!(peer_context_digest(&a), peer_context_digest(&c));
}

#[test]
fn target_tuple_parses_strict_triples() {
    let parsed = TargetTuple::parse("linux-x86_64-gnu").unwrap();
    assert_eq!(parsed.canonical(), "linux-x86_64-gnu");
    assert!(TargetTuple::parse("linux-x86_64").is_none());
    assert!(TargetTuple::parse("linux--gnu").is_none());
    assert!(TargetTuple::parse("").is_none());
}

#[test]
fn claim_specificity_ranks_exact_over_wildcards() {
    assert_eq!(claim_specificity("corp-foo"), 4);
    assert_eq!(claim_specificity("@corp/*"), 3);
    assert!(claim_matches("@corp/*", "@corp/foo"));
    assert_eq!(claim_specificity("corp-*"), 2);
    assert_eq!(claim_specificity("*-corp"), 1);
    assert_eq!(claim_specificity("*"), 0);
    assert_eq!(claim_specificity("a-*-b-*"), 1);
}

#[test]
fn claim_matches_covers_shapes() {
    assert!(claim_matches("*", "anything"));
    assert!(claim_matches("foo", "foo"));
    assert!(!claim_matches("foo", "foobar"));
    assert!(claim_matches("corp-*", "corp-foo"));
    assert!(!claim_matches("corp-*", "other-foo"));
    assert!(claim_matches("*-corp", "foo-corp"));
    assert!(claim_matches("@corp/*", "@corp/foo"));
    assert!(!claim_matches("@corp/*", "@other/foo"));
    assert!(claim_matches("foo-*-bar", "foo-x-bar"));
    assert!(!claim_matches("foo-*-bar", "foo-x-baz"));
}

fn source(id: &str, url: &str, priority: u64, claims: &[&str], trusted: bool) -> SourceRef {
    SourceRef {
        id: id.to_string(),
        url: url.to_string(),
        ecosystem: EcosystemTag::Python,
        priority,
        claims: claims.iter().map(|s| s.to_string()).collect(),
        trusted,
        allow_hosts: Vec::new(),
        allow_cidrs: Vec::new(),
        allow_protocols: vec!["https".to_string()],
    }
}

#[test]
fn specific_claim_beats_catch_all_fallback() {
    // Design §5.2 example: `corp-foo` matches BOTH `corp-*` (internal)
    // and `*` (pypi) — the specific claim MUST win, never Ambiguous.
    let sources = vec![
        source(
            "internal",
            "https://nexus.corp/simple",
            10,
            &["corp-*"],
            true,
        ),
        source("pypi", "https://pypi.org/simple", 100, &["*"], false),
    ];
    assert_eq!(select_source("corp-foo", &sources).unwrap().id, "internal");
    assert_eq!(select_source("requests", &sources).unwrap().id, "pypi");
}

#[test]
fn equal_specificity_across_sources_is_ambiguous() {
    let sources = vec![
        source("a", "https://a.example/simple", 10, &["corp-*"], false),
        source("b", "https://b.example/simple", 20, &["corp-*"], false),
    ];
    assert_eq!(
        select_source("corp-foo", &sources).unwrap_err(),
        SourceSelectionError::AmbiguousSource {
            package: "corp-foo".to_string()
        }
    );
}

#[test]
fn fallback_priority_breaks_catch_all_ties() {
    let sources = vec![
        source("mirror", "https://mirror.example/simple", 50, &["*"], false),
        source("pypi", "https://pypi.org/simple", 100, &["*"], false),
    ];
    assert_eq!(select_source("requests", &sources).unwrap().id, "mirror");
}

#[test]
fn duplicate_fallback_priority_is_ambiguous() {
    let sources = vec![
        source("a", "https://a.example/simple", 100, &["*"], false),
        source("b", "https://b.example/simple", 100, &["*"], false),
    ];
    assert_eq!(
        select_source("requests", &sources).unwrap_err(),
        SourceSelectionError::AmbiguousFallback {
            package: "requests".to_string()
        }
    );
}

#[test]
fn public_fallback_loses_to_any_trusted_match() {
    // Fallback-level anti-confusion: the public catch-all wins on
    // priority but a trusted catch-all also matches → misconfiguration,
    // failed loudly instead of resolved quietly.
    let sources = vec![
        source("internal", "https://nexus.corp/simple", 100, &["*"], true),
        source("pypi", "https://pypi.org/simple", 50, &["*"], false),
    ];
    assert_eq!(
        select_source("requests", &sources).unwrap_err(),
        SourceSelectionError::PublicOverridePrivate {
            package: "requests".to_string(),
            source: "pypi".to_string(),
        }
    );
}

#[test]
fn trusted_specific_always_wins_outright() {
    let tied = vec![
        source(
            "internal",
            "https://nexus.corp/simple",
            100,
            &["corp-*"],
            true,
        ),
        source("pypi", "https://pypi.org/simple", 100, &["*"], false),
    ];
    assert_eq!(select_source("corp-foo", &tied).unwrap().id, "internal");
    let both_specific = vec![
        source(
            "internal",
            "https://nexus.corp/simple",
            10,
            &["corp-foo"],
            true,
        ),
        source(
            "evil",
            "https://evil.example/simple",
            5,
            &["corp-foo"],
            false,
        ),
    ];
    // Same max specificity (exact) on two sources → ambiguous first.
    assert!(matches!(
        select_source("corp-foo", &both_specific).unwrap_err(),
        SourceSelectionError::AmbiguousSource { .. }
    ));
}
