//! `manifest_test.rs` — go.mod read-only parser tests (P1 Go lane).
//! Block require, single-line require, `// indirect` comments — versions
//! must stay clean of comment text. Since the Phase 2 native Go engine the
//! manifest keeps FULL module paths (PackageName repo-path form) and
//! applies the project's own `replace`/`exclude` directives — the
//! effective module graph.
//! Test parser chỉ-đọc go.mod: block require, require một dòng, comment
//! `// indirect` — version không dính văn bản comment. Từ engine Go native
//! Phase 2, manifest giữ NGUYÊN path module (dạng repo-path của
//! PackageName) và áp directive `replace`/`exclude` của project — graph
//! module hiệu lực.

#![allow(clippy::unwrap_used)]

use crate::manifest::parse_go_mod_manifest;
use crate::manifest::{
    parse_cargo_manifest, parse_pyproject_manifest, write_cargo_manifest, write_pyproject_manifest,
};

fn write_go_mod(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join("go.mod"), content).unwrap();
}

#[test]
fn single_line_require_with_indirect_comment_keeps_clean_version() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        "module example.com/m\n\ngo 1.26\n\nrequire golang.org/x/text v0.3.2 // indirect\n",
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    // Full module path is the canonical dep name (native engine resolves
    // through it).
    // (Path module đầy đủ là tên dep canonical — engine native resolve qua
    // nó.)
    let dep = manifest
        .find_dep("golang.org/x/text")
        .expect("full module path is the dep name");
    // The version range must parse `0.3.2` — NOT `v0.3.2 // indirect`.
    // Range version phải parse `0.3.2` — KHÔNG PHẢI `v0.3.2 // indirect`.
    assert_eq!(
        dep.range.satisfying_version().map(|v| v.to_string()),
        Some("0.3.2".to_string()),
        "comment must not contaminate the version"
    );
}

#[test]
fn block_require_parses_every_entry_and_module_name() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/poly\n\n",
            "go 1.26\n\n",
            "require (\n",
            "\tgolang.org/x/text v0.3.2 // indirect\n",
            "\tgopkg.in/yaml.v3 v3.0.1\n",
            ")\n\n",
            "require github.com/sergi/go-diff v1.1.0\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    assert_eq!(manifest.name, "example.com/poly");
    assert!(manifest.find_dep("golang.org/x/text").is_some());
    assert!(manifest.find_dep("gopkg.in/yaml.v3").is_some());
    assert!(manifest.find_dep("github.com/sergi/go-diff").is_some());
}

#[test]
fn exclude_drops_the_pinned_require() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/ex\n\n",
            "go 1.26\n\n",
            "require (\n",
            "\tgolang.org/x/text v0.3.2\n",
            "\tgolang.org/x/bad v0.1.0\n",
            ")\n\n",
            "exclude golang.org/x/bad v0.1.0\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("golang.org/x/text").is_some());
    assert!(
        manifest.find_dep("golang.org/x/bad").is_none(),
        "excluded (module, version) pin is dropped from the graph"
    );
}

#[test]
fn replace_rewrites_the_required_pin() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/rp\n\n",
            "go 1.26\n\n",
            "require (\n",
            "\tgolang.org/x/old v1.2.0\n",
            "\texample.com/other v0.1.0\n",
            ")\n\n",
            "replace golang.org/x/old => github.com/fork/old v1.2.1\n",
            // Version-scoped replace that does NOT match the pin — no-op.
            // (Replace theo version KHÔNG khớp pin — no-op.)
            "replace example.com/other v9.9.9 => example.com/never v0.0.1\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    let replaced = manifest
        .find_dep("github.com/fork/old")
        .expect("replace rewrote the pin");
    assert_eq!(
        replaced.range.satisfying_version().map(|v| v.to_string()),
        Some("1.2.1".to_string())
    );
    assert!(manifest.find_dep("golang.org/x/old").is_none());
    // Unmatched (version-scoped) replace leaves the pin untouched.
    // (Replace (theo version) không khớp giữ nguyên pin.)
    let untouched = manifest.find_dep("example.com/other").unwrap();
    assert_eq!(
        untouched.range.satisfying_version().map(|v| v.to_string()),
        Some("0.1.0".to_string())
    );
    assert!(manifest.find_dep("example.com/never").is_none());
}

/// pyproject round-trip (C0 FIX2): star deps serialize as the bare PEP 508
/// name (never dropped), `==` pins stay exact (never widened to `>=`),
/// `>=` bounds survive untouched. A hand-written `["six"]` must survive
/// parse → write → parse — dropping it faked a manifest mutation.
/// (Round-trip pyproject: `*` thành tên trần, `==` giữ exact.)
#[test]
fn pyproject_star_and_pins_round_trip_losslessly() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"rt\"\ndependencies = [\"six\", \"attrs==23.1.0\", \"req>=2.0\"]\n",
    )
    .unwrap();
    let manifest = parse_pyproject_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("six").unwrap().range.is_star());
    write_pyproject_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("pyproject.toml")).unwrap();
    assert!(
        body.contains("\"six\""),
        "star must persist as bare name: {body}"
    );
    assert!(
        body.contains("attrs==23.1.0"),
        "exact pin must stay ==: {body}"
    );
    assert!(
        body.contains("req>=2.0"),
        "lower bound must survive: {body}"
    );
    let again = parse_pyproject_manifest(dir.path()).unwrap();
    assert!(again.find_dep("six").is_some());
    assert!(again.find_dep("attrs").is_some());
    assert!(again.find_dep("req").is_some());
}

/// Cargo round-trip (C0 FIX2): `*` is valid Cargo — it serializes back as
/// `*` instead of vanishing on the next add.
/// (Round-trip Cargo: `*` giữ lại, không mất.)
#[test]
fn cargo_star_round_trips_instead_of_vanishing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"rt\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"*\"\n",
    )
    .unwrap();
    let manifest = parse_cargo_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("serde").unwrap().range.is_star());
    write_cargo_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(body.contains("serde"), "star dep must persist: {body}");
    let again = parse_cargo_manifest(dir.path()).unwrap();
    assert!(again.find_dep("serde").is_some(), "star dep must re-parse");
}
