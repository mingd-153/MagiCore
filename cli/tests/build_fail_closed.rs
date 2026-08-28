#![allow(clippy::unwrap_used)]

// Build contract regression tests — kiểm tra build không báo thành công giả.
mod common;

#[test]
fn ai_build_fails_instead_of_reporting_a_skipped_success() {
    let dir = common::work_dir();
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"ai-test\"\nversion = \"0.1.0\"\necosystem = \"ai\"\n\n[ai]\nframework = \"python-agent\"\n",
    )
    .unwrap();

    let (ok, out) = common::mgc_in(&dir, &["build"]);
    assert!(
        !ok,
        "AI build must not report success without an artifact: {out}"
    );
    assert!(
        out.contains("not supported for core 'ai'"),
        "unexpected output: {out}"
    );
}

#[test]
fn godot_build_fails_until_export_is_implemented() {
    let dir = common::work_dir();
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"game-test\"\nversion = \"0.1.0\"\necosystem = \"game\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .unwrap();

    let (ok, out) = common::mgc_in(&dir, &["build"]);
    assert!(!ok, "Godot build must not report a skipped success: {out}");
    assert!(out.contains("game/godot"), "unexpected output: {out}");
}
