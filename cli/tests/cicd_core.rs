// CICD core command surface: deploy multi-target + verb hints (offline tests).
mod common;

#[test]
fn test_deploy_without_cicd_project_fails() {
    let dir = common::work_dir();
    // Không có mg.toml → deploy rơi về cloud core (lỗi khác) → cần project cicd rõ ràng.
    let (ok, out) = common::mg_in(&dir, &["deploy"]);
    assert!(!ok, "deploy outside any project must fail");
    assert!(out.contains("cloud"), "expected a detection error, got: {out}");

    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mg.toml"),
        "name = \"ci-test\"\nversion = \"0.1.0\"\necosystem = \"cicd\"\n\n[cicd]\nprovider = \"github-actions\"\n",
    )
    .unwrap();
    let (ok, out) = common::mg_in(&dir, &["deploy"]);
    assert!(!ok, "deploy with cicd project but no target must fail");
    assert!(
        out.contains("CI-only"),
        "expected CI-only hint for github-actions, got: {out}"
    );
}

#[test]
fn test_cicd_verbs_hint_direction() {
    let dir = common::work_dir();
    let (ok, out) = common::mg_in(&dir, &["add-cicd", "somepkg"]);
    assert!(!ok, "add-cicd must fail");
    assert!(
        out.contains("mg deploy"),
        "expected deploy direction hint, got: {out}"
    );
}