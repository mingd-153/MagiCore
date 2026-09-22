//! Concurrent remove journal race (P0-B wiring proof): two REAL `mgc`
//! processes removing DIFFERENT deps from ONE web project at the same
//! time must serialize on the project writer lock — both succeed, the
//! final manifest lacks both deps, no journal is left behind, and no
//! spurious crash-recovery resurrects anything. Fully hermetic
//! (`--no-install`: no registry, no network).
//!
//! (Đua journal remove 2 process THẬT: xếp hàng qua lock writer — cả hai
//! xong, manifest mất cả hai dep, không journal sót, không phục hồi ma.)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    std::env::var("CARGO_BIN_EXE_mgc")
        .expect("CARGO_BIN_EXE_mgc not set — run via `cargo test -p mgc`")
}

fn create_web_project(temp: &TempDir) -> PathBuf {
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": { "is-odd": "3.0.1", "is-even": "1.0.0" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    project
}

fn remove_cmd(mgc: &str, project: &Path, package: &str, log_dir: &Path, tag: &str) -> Command {
    let stdout = std::fs::File::create(log_dir.join(format!("{tag}.out"))).unwrap();
    let stderr = std::fs::File::create(log_dir.join(format!("{tag}.err"))).unwrap();
    let mut cmd = Command::new(mgc);
    cmd.arg("--core")
        .arg("web")
        .arg("remove")
        .arg(package)
        .arg("--no-install")
        .current_dir(project)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .stdout(stdout)
        .stderr(stderr);
    cmd
}

fn read_log(log_dir: &Path, tag: &str, ext: &str) -> String {
    std::fs::read_to_string(log_dir.join(format!("{tag}.{ext}"))).unwrap_or_default()
}

#[test]
fn concurrent_removes_serialize_without_journal_corruption() {
    let temp = TempDir::new().unwrap();
    let project = create_web_project(&temp);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Launch both at once — the writer lock serializes them.
    // (Chạy cùng lúc — lock writer xếp hàng.)
    let mut first = remove_cmd(&mgc, &project, "is-odd", &log_dir, "first")
        .spawn()
        .unwrap();
    let mut second = remove_cmd(&mgc, &project, "is-even", &log_dir, "second")
        .spawn()
        .unwrap();
    let first_status = first.wait().unwrap();
    let second_status = second.wait().unwrap();
    assert!(
        first_status.success(),
        "first remove must succeed:\n{}",
        read_log(&log_dir, "first", "err")
    );
    assert!(
        second_status.success(),
        "second remove must succeed:\n{}",
        read_log(&log_dir, "second", "err")
    );

    // Both deps gone, journal gone, no phantom recovery.
    // (Mất cả hai dep, hết journal, không phục hồi ma.)
    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        !manifest.contains("is-odd"),
        "is-odd must stay removed:\n{manifest}"
    );
    assert!(
        !manifest.contains("is-even"),
        "is-even must stay removed:\n{manifest}"
    );
    assert!(
        !project
            .join(".magicore/journal/remove/journal.json")
            .exists(),
        "no stale journal may survive two serialized removes"
    );
    for tag in ["first", "second"] {
        let combined = read_log(&log_dir, tag, "out") + &read_log(&log_dir, tag, "err");
        assert!(
            !combined.contains("recovered an interrupted remove"),
            "{tag}: no spurious crash recovery on a serialized run"
        );
        assert!(
            !combined.contains("nested remove detected"),
            "{tag}: no nested-remove false positive across processes"
        );
    }
}
