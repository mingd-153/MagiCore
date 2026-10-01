use super::detect_ecosystem_at;

#[test]
fn invalid_core_marker_blocks_manifest_fallback() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join(".mgc.core"), "not-a-core\n").unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();

    let error = detect_ecosystem_at(project.path()).unwrap_err();
    assert!(
        error.to_string().contains("invalid core marker")
            && !error.to_string().contains("signature")
    );
}

#[test]
fn invalid_project_config_blocks_manifest_fallback() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("mgc.toml"), "[ecosystem\n").unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();

    assert!(detect_ecosystem_at(project.path()).is_err());
}

#[test]
fn unreadable_or_malformed_lock_does_not_fall_through_to_another_manifest() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("mgc.lock"), "not a lockfile\n").unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();

    assert!(detect_ecosystem_at(project.path()).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_core_marker_blocks_manifest_fallback() {
    let project = tempfile::tempdir().unwrap();
    let outside = project.path().join("outside-marker");
    std::fs::write(&outside, "web\n").unwrap();
    std::os::unix::fs::symlink(&outside, project.path().join(".mgc.core")).unwrap();
    std::fs::write(project.path().join("Cargo.toml"), "").unwrap();

    let error = detect_ecosystem_at(project.path()).unwrap_err();
    assert!(error.to_string().contains("symlink"));
}

#[cfg(unix)]
#[test]
fn dangling_core_marker_blocks_manifest_fallback() {
    let project = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        project.path().join("missing-marker-target"),
        project.path().join(".mgc.core"),
    )
    .unwrap();
    std::fs::write(project.path().join("Cargo.toml"), "").unwrap();

    let error = detect_ecosystem_at(project.path()).unwrap_err();
    assert!(error.to_string().contains("symlink"));
}

#[cfg(unix)]
#[test]
fn symlinked_lockfile_blocks_manifest_fallback() {
    let project = tempfile::tempdir().unwrap();
    let outside = project.path().join("outside-lock");
    std::fs::write(&outside, "version = \"3\"\n").unwrap();
    std::os::unix::fs::symlink(&outside, project.path().join("mgc.lock")).unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();

    let error = detect_ecosystem_at(project.path()).unwrap_err();
    assert!(error.to_string().contains("symlink"));
}

#[test]
fn mixed_manifests_are_ambiguous_instead_of_web_first() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();
    std::fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let error = detect_ecosystem_at(project.path()).unwrap_err();
    assert!(error.to_string().contains("Ambiguous project core"));
    assert!(!project.path().join("mgc.toml").exists());
}

#[test]
fn single_manifest_detection_does_not_write_project_configuration() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();

    assert_eq!(
        detect_ecosystem_at(project.path()).unwrap().as_deref(),
        Some("web")
    );
    assert!(!project.path().join("mgc.toml").exists());
}

#[test]
fn malformed_native_manifest_blocks_core_guessing() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("package.json"), "{not-json").unwrap();

    assert!(detect_ecosystem_at(project.path()).is_err());
}

#[test]
fn conflicting_legacy_manifest_core_declarations_fail_closed() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"name":"fixture","magicore":{"core":"web"}}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n[package.metadata.magicore]\ncore = \"game\"\n",
    )
    .unwrap();

    let error = detect_ecosystem_at(project.path()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("conflicting MagiCore core declarations")
    );
}

#[test]
fn legacy_cloud_manifest_hint_uses_canonical_core_name() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"name":"fixture","magicore":{"core":"Cloud"}}"#,
    )
    .unwrap();

    assert_eq!(
        detect_ecosystem_at(project.path()).unwrap().as_deref(),
        Some("clo")
    );
}
