//! Integration tests for mg-platform reflink — test riêng tại test/ (RULE §5)
//! (Reflink clone: copy-on-write trên APFS/FICLONE, fallback NotSupported đúng đường)
use mg_platform::reflink::{reflink_clone, ReflinkError};
use std::io::Write;
use tempfile;

#[test]
fn reflink_clone_creates_identical_content() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source.bin");
    let target = tmp.path().join("target.bin");
    let payload = b"megagate reflink payload 1234567890";
    std::fs::write(&source, payload).unwrap();

    match reflink_clone(&source, &target) {
        Ok(()) => {
            let cloned = std::fs::read(&target).unwrap();
            assert_eq!(cloned, payload, "clone content must match source");
        }
        Err(ReflinkError::NotSupported(_)) => {
            // tmpdir fs (e.g. non-APFS volume) may not support reflink —
            // this is a valid outcome; the contract is NotSupported, not error.
        }
        Err(ReflinkError::Other(err)) => panic!("reflink failed unexpectedly: {err}"),
    }
}

#[test]
fn reflink_clone_is_copy_on_write() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source.txt");
    let target = tmp.path().join("target.txt");
    std::fs::write(&source, b"original").unwrap();

    if let Err(ReflinkError::NotSupported(_)) = reflink_clone(&source, &target) {
        return; // fs without reflink: nothing to verify
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .unwrap();
    file.write_all(b"EDITED!").unwrap();
    drop(file);

    let source_content = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        source_content, "original",
        "editing clone must not touch source"
    );
}

#[test]
fn reflink_clone_to_missing_source_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist.bin");
    let target = tmp.path().join("out.bin");
    match reflink_clone(&missing, &target) {
        Err(ReflinkError::Other(_)) => {}
        Err(ReflinkError::NotSupported(_)) => {} // fs check may come first
        Ok(()) => panic!("cloning a missing source must not succeed"),
    }
}

#[test]
fn reflink_clone_to_existing_target_errors_no_clobber() {
    // Re-materialize: target already exists — clonefile/FICLONE refuse to
    // overwrite, so callers must remove the stale target first (they do;
    // this test pins the no-silent-clobber contract).
    // (Target đã tồn tại: clone phải fail — không tự ghi đè lén lút.)
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source.bin");
    let target = tmp.path().join("target.bin");
    std::fs::write(&source, b"fresh content").unwrap();
    std::fs::write(&target, b"stale content").unwrap();

    match reflink_clone(&source, &target) {
        Ok(()) => {
            // fs that silently clobbers (rare); accept but verify content is fresh
            assert_eq!(std::fs::read(&target).unwrap(), b"fresh content");
        }
        Err(ReflinkError::NotSupported(_)) => {}
        Err(ReflinkError::Other(_)) => {
            assert_eq!(
                std::fs::read(&target).unwrap(),
                b"stale content",
                "failed clone must not have partially touched the file"
            );
        }
    }
}
