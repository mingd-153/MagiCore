use super::*;

#[test]
fn hardware_kind_accepts_optimizer_and_bench() {
    assert!(hardware_kind(OPTIMIZER_PKG).is_ok());
    assert!(hardware_kind(BENCH_PKG).is_ok());
}

#[test]
fn hardware_kind_rejects_unknown_package() {
    let err = hardware_kind("nonsense").unwrap_err();
    assert!(err.to_string().contains("unknown hardware package"));
}

#[tokio::test]
async fn add_with_version_pin_fails_loudly() {
    // T0.3-version: templates have no versions — the pin must fail
    // loudly, never drop silently (checked before any filesystem
    // access, so no fixture is needed).
    let err = add(
        vec!["optimizer".to_string()],
        None,
        Some("1.0.0".to_string()),
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("--version"),
        "unexpected error: {err}"
    );
}

#[test]
fn optimizer_owner_uses_project_config_when_marker_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    let config = mgc_config::project::ProjectConfig::new("ai-owner", "ai");
    config.save(dir.path()).unwrap();
    std::fs::remove_file(
        dir.path()
            .join(mgc_config::project::ProjectConfig::CORE_MARKER_FILE),
    )
    .unwrap();

    assert_eq!(optimizer_owner(dir.path()).unwrap(), "ai");
}

#[test]
fn optimizer_owner_refuses_missing_or_ambiguous_identity() {
    let empty = tempfile::tempdir().unwrap();
    let missing = optimizer_owner(empty.path()).unwrap_err();
    assert!(
        missing
            .to_string()
            .contains("cannot prove project core identity")
    );

    let ambiguous = tempfile::tempdir().unwrap();
    std::fs::write(ambiguous.path().join("package.json"), "{}\n").unwrap();
    std::fs::write(
        ambiguous.path().join("Cargo.toml"),
        "[package]\nname = \"ambiguous\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let error = optimizer_owner(ambiguous.path()).unwrap_err();
    assert!(error.to_string().contains("Ambiguous project core"));
}
