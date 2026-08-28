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
    assert!(content.contains("LINE2"));
    assert!(!content.contains("line2"));
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
