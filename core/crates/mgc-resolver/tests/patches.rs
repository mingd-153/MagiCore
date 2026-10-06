#![allow(clippy::unwrap_used)]
//! Integration tests for patch apply engine — test riêng tại test/ (RULE §5)
use mgc_resolver::patches::{apply_patch, verify_patch_integrity};

#[test]
fn apply_simple_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let vstore = tmp.path();

    // Create original file
    let orig = vstore.join("test.txt");
    std::fs::write(&orig, "line1\nline2\nline3\n").unwrap();

    // Create patch
    let patch = vstore.join("test.patch");
    std::fs::write(
        &patch,
        r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line1
-line2
+LINE2
 line3
"#,
    )
    .unwrap();

    let modified = apply_patch(vstore, &patch).unwrap();
    assert_eq!(modified.len(), 1);
    let content = std::fs::read_to_string(&orig).unwrap();
    assert_eq!(content, "line1\nLINE2\nline3\n");
}

#[test]
fn apply_multiple_hunks_in_one_file() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("test.txt");
    std::fs::write(&target, "one\ntwo\nthree\nfour\nfive\nsix\n").unwrap();
    let patch = tmp.path().join("multi.patch");
    std::fs::write(
        &patch,
        "--- a/test.txt\n+++ b/test.txt\n@@ -1,1 +1,1 @@\n-one\n+ONE\n@@ -6,1 +6,1 @@\n-six\n+SIX\n",
    )
    .unwrap();

    apply_patch(tmp.path(), &patch).unwrap();
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "ONE\ntwo\nthree\nfour\nfive\nSIX\n"
    );
}

#[test]
fn apply_multiple_files_and_validate_all_before_writing_any_file() {
    let tmp = tempfile::tempdir().unwrap();
    let first = tmp.path().join("first.txt");
    let second = tmp.path().join("second.txt");
    std::fs::write(&first, "first\n").unwrap();
    std::fs::write(&second, "second\n").unwrap();
    let patch = tmp.path().join("multi-file.patch");
    std::fs::write(
        &patch,
        "--- a/first.txt\n+++ b/first.txt\n@@ -1,1 +1,1 @@\n-first\n+FIRST\n--- a/second.txt\n+++ b/second.txt\n@@ -1,1 +1,1 @@\n-wrong\n+SECOND\n",
    )
    .unwrap();

    assert!(apply_patch(tmp.path(), &patch).is_err());
    assert_eq!(std::fs::read_to_string(first).unwrap(), "first\n");
    assert_eq!(std::fs::read_to_string(second).unwrap(), "second\n");
}

#[test]
fn malformed_unicode_hunk_line_returns_error_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("test.txt"), "one\n").unwrap();
    let patch = tmp.path().join("unicode.patch");
    std::fs::write(
        &patch,
        "--- a/test.txt\n+++ b/test.txt\n@@ -1,1 +1,1 @@\nébad\n",
    )
    .unwrap();

    let result = std::panic::catch_unwind(|| apply_patch(tmp.path(), &patch));
    assert!(
        result.is_ok(),
        "malformed Unicode must not panic the process"
    );
    assert!(result.unwrap().is_err());
}

#[test]
fn overflowing_hunk_range_returns_error_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("test.txt"), "one\n").unwrap();
    let patch = tmp.path().join("overflow.patch");
    let patch_text = format!(
        "--- a/test.txt\n+++ b/test.txt\n@@ -2,{} +1,1 @@\n-one\n+ONE\n",
        usize::MAX
    );
    std::fs::write(&patch, patch_text).unwrap();

    let result = std::panic::catch_unwind(|| apply_patch(tmp.path(), &patch));
    assert!(result.is_ok(), "malformed hunk range must not panic");
    assert!(result.unwrap().is_err());
}

#[cfg(unix)]
#[test]
fn apply_patch_rejects_symlink_target_escaping_vstore() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let vstore = tmp.path().join("vstore");
    std::fs::create_dir(&vstore).unwrap();
    let outside = tmp.path().join("outside.txt");
    std::fs::write(&outside, "keep-me\n").unwrap();
    symlink(&outside, vstore.join("target.txt")).unwrap();
    let patch = tmp.path().join("symlink.patch");
    std::fs::write(
        &patch,
        "--- a/target.txt\n+++ b/target.txt\n@@ -1,1 +1,1 @@\n-keep-me\n+changed\n",
    )
    .unwrap();

    assert!(apply_patch(&vstore, &patch).is_err());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep-me\n");
}

#[test]
fn patch_context_mismatch_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let vstore = tmp.path();

    let orig = vstore.join("test.txt");
    std::fs::write(&orig, "line1\nline2\nline3\n").unwrap();

    let patch = vstore.join("test.patch");
    std::fs::write(
        &patch,
        r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line1
-lineX
+LINE2
 line3
"#,
    )
    .unwrap();

    let result = apply_patch(vstore, &patch);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("context mismatch"));
}

#[cfg(unix)]
#[test]
fn apply_patch_rejects_parent_path_escape_without_touching_outside_file() {
    let tmp = tempfile::tempdir().unwrap();
    let vstore = tmp.path().join("vstore");
    std::fs::create_dir(&vstore).unwrap();
    let outside = tmp.path().join("outside.txt");
    std::fs::write(&outside, "keep-me\n").unwrap();

    let patch = tmp.path().join("escape.patch");
    std::fs::write(
        &patch,
        "--- a/../outside.txt\n+++ b/../outside.txt\n@@ -1,1 +1,1 @@\n-keep-me\n+owned-by-patch\n",
    )
    .unwrap();

    let result = apply_patch(&vstore, &patch);
    assert!(result.is_err(), "parent traversal must be rejected");
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep-me\n");
}

#[test]
fn verify_patch_integrity_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let patch = tmp.path().join("test.patch");
    std::fs::write(&patch, "test content").unwrap();
    let sha = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"test content");
        hex::encode(h.finalize())
    };
    assert!(verify_patch_integrity(&patch, &sha).unwrap());
}

#[test]
fn verify_patch_integrity_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let patch = tmp.path().join("test.patch");
    std::fs::write(&patch, "test content").unwrap();
    assert!(!verify_patch_integrity(&patch, "wrong").unwrap());
}

#[test]
fn verify_patch_integrity_sri_prefixed() {
    // `mgc patch add` records SRI form (`sha256-<hex>`) — verification
    // must accept it (bare-hex-only comparison rejected every added patch).
    let tmp = tempfile::tempdir().unwrap();
    let patch = tmp.path().join("test.patch");
    std::fs::write(&patch, "test content").unwrap();
    let sha = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"test content");
        format!("sha256-{}", hex::encode(h.finalize()))
    };
    assert!(verify_patch_integrity(&patch, &sha).unwrap());
}
