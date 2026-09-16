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
