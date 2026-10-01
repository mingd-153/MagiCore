#![allow(clippy::unwrap_used)]
//! T9a plain-text core marker tests — ngăn nhầm core, không phải chữ ký mật mã.
//! Covers: write/read marker, save() auto-writes marker, detect priority,
//! ambiguous fail-closed, find_project_root marker propagation.

use mgc_config::project::ProjectConfig;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mgc-config-marker-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn project(name: &str) -> PathBuf {
    let dir = tmp_dir(name);
    let cfg = ProjectConfig::new(dir.file_name().unwrap().to_string_lossy().as_ref(), "web");
    cfg.save(&dir).unwrap();
    dir
}

#[test]
fn save_writes_marker_with_ecosystem() {
    let dir = project("saves-marker");
    let marker = dir.join(ProjectConfig::CORE_MARKER_FILE);
    assert!(marker.exists(), "save() must write .mgc.core");
    let content = fs::read_to_string(&marker).unwrap();
    assert_eq!(content.trim(), "web");
}

#[test]
fn read_marker_roundtrip_and_cloud_alias() {
    let dir = tmp_dir("alias-cloud");
    ProjectConfig::write_core_marker_at(&dir, "cloud").unwrap();
    let core = ProjectConfig::read_core_marker(&dir).unwrap();
    assert_eq!(
        core.as_deref(),
        Some("clo"),
        "cloud must canonicalize to clo"
    );
}

#[test]
fn signature_write_cannot_reassign_configured_project_core() {
    let dir = project("signature-reassign");
    let marker_path = dir.join(ProjectConfig::CORE_MARKER_FILE);
    let config_path = dir.join("mgc.toml");
    let marker_before = fs::read(&marker_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();

    let err = ProjectConfig::write_core_marker_at(&dir, "ai").unwrap_err();

    assert!(err.to_string().contains("already configured as core 'web'"));
    assert_eq!(fs::read(&marker_path).unwrap(), marker_before);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
}

#[test]
fn project_identity_restore_recovers_exact_marker_and_config_bytes() {
    let dir = project("identity-restore");
    let marker_path = dir.join(ProjectConfig::CORE_MARKER_FILE);
    let config_path = dir.join("mgc.toml");
    let snapshot = mgc_config::project::snapshot_project_identity(&dir).unwrap();
    let marker_before = fs::read(&marker_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();

    fs::write(&marker_path, "ai\n").unwrap();
    fs::write(&config_path, "not valid TOML").unwrap();
    mgc_config::project::restore_project_identity(&dir, &snapshot).unwrap();

    assert_eq!(fs::read(&marker_path).unwrap(), marker_before);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("web")
    );
}

#[test]
fn project_identity_restore_preserves_non_identity_config_edits() {
    let dir = project("identity-preserve-config-edits");
    let config_path = dir.join("mgc.toml");
    let snapshot = mgc_config::project::snapshot_project_identity(&dir).unwrap();
    let mut edited_config = fs::read(&config_path).unwrap();
    edited_config.extend_from_slice(b"\n# user-owned lifecycle change\n");
    fs::write(&config_path, &edited_config).unwrap();

    assert!(mgc_config::project::project_identity_matches_snapshot(&dir, &snapshot).unwrap());
    mgc_config::project::restore_project_identity(&dir, &snapshot).unwrap();

    assert_eq!(
        fs::read(&config_path).unwrap(),
        edited_config,
        "identity recovery must not discard unrelated config edits"
    );
}

#[test]
fn project_identity_match_rejects_same_core_marker_rewrite() {
    let dir = project("identity-same-core-marker-rewrite");
    let marker_path = dir.join(ProjectConfig::CORE_MARKER_FILE);
    let snapshot = mgc_config::project::snapshot_project_identity(&dir).unwrap();
    fs::write(&marker_path, "web").unwrap();

    assert!(!mgc_config::project::project_identity_matches_snapshot(&dir, &snapshot).unwrap());
}

#[cfg(unix)]
#[test]
fn project_identity_restore_refuses_symlink_targets() {
    let dir = project("identity-restore-symlink");
    let outside = tmp_dir("identity-restore-outside").join("target");
    fs::write(&outside, "do not overwrite").unwrap();
    let snapshot = mgc_config::project::snapshot_project_identity(&dir).unwrap();
    fs::remove_file(dir.join(ProjectConfig::CORE_MARKER_FILE)).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join(ProjectConfig::CORE_MARKER_FILE)).unwrap();

    let error = mgc_config::project::restore_project_identity(&dir, &snapshot).unwrap_err();

    assert!(error.to_string().contains("must not be a symlink"));
    assert_eq!(fs::read_to_string(outside).unwrap(), "do not overwrite");
}

#[test]
fn public_marker_helpers_cannot_reassign_configured_project_core() {
    let dir = project("public-marker-helper-reassign");
    let marker_path = dir.join(ProjectConfig::CORE_MARKER_FILE);
    let config_path = dir.join("mgc.toml");
    let marker_before = fs::read(&marker_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();
    let replacement = ProjectConfig::new("public-marker-helper-reassign", "ai");

    let write_err = replacement.write_core_marker(&dir).unwrap_err();
    let ensure_err = ProjectConfig::ensure_core_marker_at(&dir, "ai").unwrap_err();

    assert!(
        write_err
            .to_string()
            .contains("already configured as core 'web'")
    );
    assert!(
        ensure_err
            .to_string()
            .contains("already configured as core 'web'")
    );
    assert_eq!(fs::read(&marker_path).unwrap(), marker_before);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
}

#[test]
fn read_marker_invalid_core_fails_closed() {
    let dir = tmp_dir("invalid-core");
    let marker_path = dir.join(ProjectConfig::CORE_MARKER_FILE);
    fs::write(&marker_path, "not-a-core\n").unwrap();
    let original = fs::read(&marker_path).unwrap();
    let err = ProjectConfig::read_core_marker(&dir).unwrap_err();
    assert!(
        err.to_string().contains("invalid core marker") && !err.to_string().contains("signature"),
        "expected fail-closed on bad marker, got: {err}"
    );
    assert!(err.to_string().contains("Repair the marker file manually"));
    let write_err = ProjectConfig::write_core_marker_at(&dir, "web").unwrap_err();
    assert!(
        write_err.to_string().contains("invalid core marker")
            && !write_err.to_string().contains("signature")
    );
    assert_eq!(fs::read(&marker_path).unwrap(), original);
}

#[test]
fn detect_core_priority_marker_over_signature() {
    let dir = project("priority-marker");
    let marker = fs::read_to_string(dir.join(ProjectConfig::CORE_MARKER_FILE)).unwrap();
    assert_eq!(marker.trim(), "web");
    // ngay cả khi thêm signature khác core — marker vẫn thắng
    fs::write(dir.join("package.json"), "{}").unwrap();
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("web")
    );
}

#[test]
fn detect_core_rejects_marker_that_conflicts_with_project_config() {
    let dir = project("marker-config-conflict");
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "ai\n").unwrap();

    let error = ProjectConfig::detect_core(&dir).unwrap_err();

    assert!(error.to_string().contains("conflicts with mgc.toml"));
}

#[test]
fn auto_detect_preserves_core_identity_errors() {
    let dir = project("auto-detect-core-conflict");
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "ai\n").unwrap();

    let result = ProjectConfig::auto_detect(&dir);

    assert!(
        result.is_err(),
        "auto_detect must propagate a conflicting identity instead of converting it to no core: {result:?}"
    );
}

#[test]
fn read_marker_rejects_conflict_with_project_config() {
    let dir = project("read-marker-config-conflict");
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "ai\n").unwrap();

    let error = ProjectConfig::read_core_marker(&dir).unwrap_err();

    assert!(error.to_string().contains("conflicts with mgc.toml"));
}

#[test]
fn detect_core_accepts_cloud_alias_when_marker_and_config_agree() {
    let dir = tmp_dir("marker-config-cloud-alias");
    ProjectConfig::new("marker-config-cloud-alias", "cloud")
        .save(&dir)
        .unwrap();

    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("clo")
    );
}

#[test]
fn detect_core_rejects_marker_when_config_has_no_ecosystem() {
    let dir = tmp_dir("marker-config-missing-ecosystem");
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "web\n").unwrap();
    fs::write(dir.join("mgc.toml"), "name = \"broken\"\n").unwrap();

    let error = ProjectConfig::detect_core(&dir).unwrap_err();

    assert!(error.to_string().contains("missing its ecosystem"));
}

#[test]
fn detect_core_rejects_marker_when_config_has_unknown_ecosystem() {
    let dir = tmp_dir("marker-config-unknown-ecosystem");
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "web\n").unwrap();
    fs::write(
        dir.join("mgc.toml"),
        "name = \"broken\"\necosystem = \"not-a-core\"\n",
    )
    .unwrap();

    let error = ProjectConfig::detect_core(&dir).unwrap_err();

    assert!(error.to_string().contains("unknown core 'not-a-core'"));
}

#[test]
fn detect_core_signature_single() {
    let dir = tmp_dir("detect-single");
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("lib"),
        "Cargo.toml signature → lib (game/iot must have their own marker)"
    );
}

#[test]
fn detect_core_ambiguous_fails_closed() {
    let dir = tmp_dir("detect-ambiguous");
    fs::write(dir.join("package.json"), "{}").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    let err = ProjectConfig::detect_core(&dir).unwrap_err();
    assert!(
        err.to_string().contains("Ambiguous"),
        "two signatures + no marker must fail, got: {err}"
    );
    // `auto_detect` must preserve ambiguity instead of flattening it to `None`.
    // (auto_detect phải giữ lỗi mơ hồ thay vì biến thành `None`.)
    assert!(ProjectConfig::auto_detect(&dir).is_err());
}

#[test]
fn detect_core_marker_disambiguates() {
    let dir = tmp_dir("detect-disambiguate");
    fs::write(dir.join("package.json"), "{}").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    ProjectConfig::write_core_marker_at(&dir, "game").unwrap();
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("game"),
        "marker must disambiguate: a game project with Cargo.toml must not be mistaken for lib"
    );
}

#[test]
fn find_project_root_sees_marker_in_parent() {
    let dir = tmp_dir("find-root");
    let inner = dir.join("a/b/c");
    fs::create_dir_all(&inner).unwrap();
    assert_eq!(
        ProjectConfig::find_project_root(&inner),
        None,
        "no marker yet"
    );
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "iot\n").unwrap();
    assert_eq!(
        ProjectConfig::find_project_root(&inner),
        Some(dir),
        "marker in grandparent must anchor root (monorepo)"
    );
}

#[test]
fn write_marker_unknown_core_rejected() {
    let dir = tmp_dir("unknown-core");
    let err = ProjectConfig::write_core_marker_at(&dir, "monolith").unwrap_err();
    assert!(err.to_string().contains("Unknown core"), "got: {err}");
    assert!(!dir.join(ProjectConfig::CORE_MARKER_FILE).exists());
}

#[test]
fn save_refuses_cross_core_reassignment_before_mutating_project_files() {
    let dir = project("cross-core-save");
    let marker_path = dir.join(ProjectConfig::CORE_MARKER_FILE);
    let config_path = dir.join("mgc.toml");
    let marker_before = fs::read(&marker_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();
    let replacement = ProjectConfig::new(dir.file_name().unwrap().to_string_lossy().as_ref(), "ai");

    let err = replacement.save(&dir).unwrap_err();

    assert!(err.to_string().contains("already configured as core 'web'"));
    assert_eq!(fs::read(&marker_path).unwrap(), marker_before);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
}

#[test]
fn save_refuses_to_reassign_legacy_project_without_marker() {
    let dir = tmp_dir("legacy-cross-core-save");
    let config_path = dir.join("mgc.toml");
    fs::write(
        &config_path,
        toml::to_string_pretty(&ProjectConfig::new("legacy", "web")).unwrap(),
    )
    .unwrap();
    let config_before = fs::read(&config_path).unwrap();
    let replacement = ProjectConfig::new("legacy", "ai");

    let err = replacement.save(&dir).unwrap_err();

    assert!(err.to_string().contains("already configured as core 'web'"));
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
    assert!(!dir.join(ProjectConfig::CORE_MARKER_FILE).exists());
}

#[test]
fn ensure_marker_is_idempotent_for_same_core_and_refuses_another_core() {
    let dir = tmp_dir("ensure-marker");

    ProjectConfig::ensure_core_marker_at(&dir, "web").unwrap();
    let err = ProjectConfig::ensure_core_marker_at(&dir, "lib").unwrap_err();

    assert!(err.to_string().contains("already marked as core 'web'"));
    assert_eq!(
        fs::read_to_string(dir.join(ProjectConfig::CORE_MARKER_FILE)).unwrap(),
        "web\n"
    );
}

#[test]
fn concurrent_different_core_claims_publish_exactly_one_signature() {
    use std::sync::{Arc, Barrier};

    let dir = tmp_dir("concurrent-core-claims");
    let cores = ["web", "ai", "app", "lib"];
    let barrier = Arc::new(Barrier::new(cores.len()));
    let handles = cores
        .into_iter()
        .map(|core| {
            let root = dir.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                let result = ProjectConfig::write_core_marker_at(&root, core);
                (core, result)
            })
        })
        .collect::<Vec<_>>();

    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("claim worker must not panic"))
        .collect::<Vec<_>>();
    let winners = results
        .iter()
        .filter_map(|(core, result)| result.as_ref().ok().map(|_| *core))
        .collect::<Vec<_>>();

    assert_eq!(
        winners.len(),
        1,
        "exactly one different core may claim the project"
    );
    assert_eq!(
        ProjectConfig::read_core_marker(&dir).unwrap().as_deref(),
        Some(winners[0]),
        "published signature must match the only successful claimant"
    );
    for core in cores {
        let expected_success = core == winners[0];
        assert_eq!(
            results
                .iter()
                .find(|(candidate, _)| *candidate == core)
                .unwrap()
                .1
                .is_ok(),
            expected_success,
            "claim result for {core} must agree with the published signature"
        );
    }
    assert!(
        std::fs::read_dir(&dir).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".mgc.core.claim.")),
        "claim staging files must be removed after both winners and losers"
    );
}

#[cfg(unix)]
#[test]
fn core_marker_symlink_is_rejected_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tmp_dir("marker-symlink");
    let target = dir.join("outside.txt");
    let marker = dir.join(ProjectConfig::CORE_MARKER_FILE);
    fs::write(&target, "sentinel\n").unwrap();
    symlink(&target, &marker).unwrap();

    let read_err = ProjectConfig::read_core_marker(&dir).unwrap_err();
    let write_err = ProjectConfig::ensure_core_marker_at(&dir, "web").unwrap_err();
    let reassign_err = ProjectConfig::write_core_marker_at(&dir, "ai").unwrap_err();

    assert!(read_err.to_string().contains("must not be a symlink"));
    assert!(write_err.to_string().contains("must not be a symlink"));
    assert!(reassign_err.to_string().contains("must not be a symlink"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "sentinel\n");
}

#[cfg(unix)]
#[test]
fn project_config_symlink_is_rejected_before_save() {
    use std::os::unix::fs::symlink;

    let dir = tmp_dir("config-symlink");
    let target = dir.join("outside.toml");
    let config_path = dir.join("mgc.toml");
    let existing = ProjectConfig::new("outside", "web");
    fs::write(&target, toml::to_string_pretty(&existing).unwrap()).unwrap();
    let target_before = fs::read(&target).unwrap();
    symlink(&target, &config_path).unwrap();
    let replacement = ProjectConfig::new("inside", "ai");

    let err = replacement.save(&dir).unwrap_err();

    assert!(err.to_string().contains("project config") && err.to_string().contains("symlink"));
    assert_eq!(fs::read(&target).unwrap(), target_before);
    assert!(!dir.join(ProjectConfig::CORE_MARKER_FILE).exists());
}

#[cfg(unix)]
#[test]
fn project_config_load_rejects_symlink_without_reading_target() {
    use std::os::unix::fs::symlink;

    let dir = tmp_dir("config-load-symlink");
    let target = dir.join("outside.toml");
    let config_path = dir.join("mgc.toml");
    let outside = ProjectConfig::new("outside", "web");
    fs::write(&target, toml::to_string_pretty(&outside).unwrap()).unwrap();
    symlink(&target, &config_path).unwrap();

    let err = ProjectConfig::load(&dir).unwrap_err();

    assert!(err.to_string().contains("must not be a symlink"));
}

#[cfg(unix)]
#[test]
fn detect_core_rejects_symlinked_config_and_signature_files() {
    use std::os::unix::fs::symlink;

    let config_project = tmp_dir("detect-config-symlink");
    let config_target = config_project.join("outside.toml");
    fs::write(
        &config_target,
        toml::to_string_pretty(&ProjectConfig::new("outside", "web")).unwrap(),
    )
    .unwrap();
    symlink(&config_target, config_project.join("mgc.toml")).unwrap();
    assert!(
        ProjectConfig::detect_core(&config_project)
            .unwrap_err()
            .to_string()
            .contains("must not be a symlink")
    );

    let signature_project = tmp_dir("detect-signature-symlink");
    let signature_target = signature_project.join("outside-package.json");
    fs::write(&signature_target, "{}\n").unwrap();
    symlink(&signature_target, signature_project.join("package.json")).unwrap();
    assert!(
        ProjectConfig::detect_core(&signature_project)
            .unwrap_err()
            .to_string()
            .contains("project signature")
    );
    assert_eq!(fs::read_to_string(signature_target).unwrap(), "{}\n");
}
