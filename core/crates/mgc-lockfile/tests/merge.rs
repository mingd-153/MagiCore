//! Tests cho 3-way merge (v2 schema) — tách khỏi src theo RULE §5.

use mgc_lockfile::{merge3, resolve_git_conflict_markers, serialize_lockfile, Lockfile, Package};

fn pkg(name: &str, version: &str) -> Package {
    Package {
        name: name.to_string(),
        version: version.to_string(),
        resolved: format!("https://registry.example/{name}"),
        integrity: String::new(),
        dependencies: vec![],
    }
}

fn lock(packages: &[Package]) -> Lockfile {
    let mut l = Lockfile::new();
    l.packages = packages.to_vec();
    l
}

#[test]
fn merge3_keeps_both_additions() {
    let base = lock(&[]);
    let ours = lock(&[pkg("a", "1.0.0")]);
    let theirs = lock(&[pkg("b", "2.0.0")]);
    let out = merge3(&base, &ours, &theirs).unwrap();
    assert_eq!(out.packages.len(), 2);
    let names: Vec<_> = out.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"a") && names.contains(&"b"));
}

#[test]
fn merge3_keeps_common_changed_version() {
    let base = lock(&[pkg("a", "1.0.0")]);
    let ours = lock(&[pkg("a", "1.1.0")]);
    let theirs = lock(&[pkg("a", "1.1.0")]);
    let out = merge3(&base, &ours, &theirs).unwrap();
    assert_eq!(out.packages.len(), 1);
    assert_eq!(out.packages[0].version, "1.1.0");
}

#[test]
fn merge3_takes_single_side_bump() {
    let base = lock(&[pkg("a", "1.0.0")]);
    let ours = lock(&[pkg("a", "1.0.0")]); // giữ nguyên
    let theirs = lock(&[pkg("a", "2.0.0")]); // bump
    let out = merge3(&base, &ours, &theirs).unwrap();
    assert_eq!(out.packages[0].version, "2.0.0");
}

#[test]
fn merge3_conflicts_on_divergent_versions() {
    let base = lock(&[pkg("a", "1.0.0")]);
    let ours = lock(&[pkg("a", "1.1.0")]);
    let theirs = lock(&[pkg("a", "2.0.0")]);
    let err = merge3(&base, &ours, &theirs).unwrap_err();
    assert!(err.to_string().contains("conflict"), "{err}");
    assert_eq!(err.name, "a");
}

#[test]
fn merge3_removal_wins_when_other_side_unchanged() {
    let base = lock(&[pkg("a", "1.0.0"), pkg("b", "1.0.0")]);
    let ours = lock(&[pkg("b", "1.0.0")]); // removed a
    let theirs = lock(&[pkg("a", "1.0.0"), pkg("b", "1.0.0")]);
    let out = merge3(&base, &ours, &theirs).unwrap();
    assert_eq!(out.packages.len(), 1);
    assert_eq!(out.packages[0].name, "b");
}

#[test]
fn merge3_keeps_bump_over_other_side_removal() {
    let base = lock(&[pkg("a", "1.0.0")]);
    let ours = lock(&[]); // removed a
    let theirs = lock(&[pkg("a", "2.0.0")]); // bumped a → bump thắng removal
    let out = merge3(&base, &ours, &theirs).unwrap();
    assert_eq!(out.packages.len(), 1);
    assert_eq!(out.packages[0].version, "2.0.0");
}

// ------------------------------------------------- git conflict markers

const MARKERED_LOCK: &str = r#"version = "2"

[metadata]
generated_at = "2026-01-01T00:00:00Z"
generator = "mgc/test"
lockfile_hash = ""

[[package]]
name = "common"
version = "1.0.0"
resolved = "https://registry.example/common"
integrity = ""
dependencies = []

<<<<<<< ours
[[package]]
name = "ours-only"
version = "1.0.0"
resolved = "https://registry.example/ours"
integrity = ""
dependencies = []
=======
[[package]]
name = "theirs-only"
version = "2.0.0"
resolved = "https://registry.example/theirs"
integrity = ""
dependencies = []
>>>>>>> theirs
"#;

#[test]
fn conflict_markers_resolve_to_union_when_sides_parse() {
    // Cả 2 phía parse được TOML hợp lệ → trộn union (base rỗng)
    let merged = resolve_git_conflict_markers(MARKERED_LOCK).expect("should auto-resolve");
    let names: Vec<_> = merged.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"common"), "phần chung giữ lại");
    assert!(
        names.contains(&"ours-only") && names.contains(&"theirs-only"),
        "union cả 2 phía thêm mới: {names:?}"
    );
}

#[test]
fn plain_lockfile_has_no_markers_returns_none() {
    let plain = lock(&[pkg("a", "1.0.0")]);
    let text = serialize_lockfile(&plain).unwrap();
    assert!(resolve_git_conflict_markers(&text).is_none());
}
