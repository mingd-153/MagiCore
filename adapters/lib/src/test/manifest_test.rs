//! `manifest_test.rs` — go.mod read-only parser tests (P1 Go lane).
//! Block require, single-line require, `// indirect` comments, exclude
//! blocks ignored — versions must stay clean of comment text.
//! Test parser chỉ-đọc go.mod: block require, require một dòng, comment
//! `// indirect`, bỏ exclude — version không dính văn bản comment.

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
    assert!(
        manifest.find_dep("text").is_some(),
        "display name = last path segment"
    );
    let dep = manifest.find_dep("text").unwrap();
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
    assert!(manifest.find_dep("text").is_some());
    assert!(manifest.find_dep("yaml.v3").is_some());
    assert!(manifest.find_dep("go-diff").is_some());
}

#[test]
fn exclude_and_replace_blocks_are_not_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/ex\n\n",
            "go 1.26\n\n",
            "require golang.org/x/text v0.3.2\n\n",
            "exclude golang.org/x/bad v0.1.0\n",
            "replace golang.org/x/old => golang.org/x/new v1.0.0\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("text").is_some());
    assert!(manifest.find_dep("bad").is_none(), "exclude is not a dep");
    assert!(manifest.find_dep("old").is_none(), "replace is not a dep");
    assert!(
        manifest.find_dep("new").is_none(),
        "replace target is not a dep"
    );
}
