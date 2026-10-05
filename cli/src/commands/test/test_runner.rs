// Test-runner selection contracts — khóa lựa chọn runner theo ownership.
use super::{detect_test_core, detect_test_runner, resolve_mgc_toml_script};

#[test]
fn flutter_test_disables_implicit_pub_resolution() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pubspec.yaml"), "name: fixture\n").unwrap();

    let (runner, args) = detect_test_runner(dir.path()).unwrap().unwrap();
    assert_eq!(runner, "flutter");
    assert_eq!(args, ["test", "--no-pub"]);
}

#[test]
fn swift_tests_disable_package_updates_and_automatic_resolution() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Package.swift"),
        "// swift-tools-version: 5.9\nlet package = Package(name: \"fixture\")\n",
    )
    .unwrap();

    let (runner, args) = detect_test_runner(dir.path()).unwrap().unwrap();
    assert_eq!(runner, "swift");
    assert_eq!(
        args,
        ["test", "--skip-update", "--disable-automatic-resolution"]
    );
}

#[test]
fn rust_and_go_tests_are_locked_offline() {
    let rust = tempfile::tempdir().unwrap();
    std::fs::write(
        rust.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    let (runner, args) = detect_test_runner(rust.path()).unwrap().unwrap();
    assert_eq!(runner, "cargo");
    assert!(args.contains(&"--locked".to_string()));
    assert!(args.contains(&"--offline".to_string()));

    let go = tempfile::tempdir().unwrap();
    std::fs::write(
        go.path().join("go.mod"),
        "module example.test/fixture\n\ngo 1.22\n",
    )
    .unwrap();
    let (runner, args) = detect_test_runner(go.path()).unwrap().unwrap();
    assert_eq!(runner, "go");
    assert!(args.contains(&"-mod=readonly".to_string()));
}

#[test]
fn core_detection_uses_shared_marker_parser_and_accepts_its_comment_format() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".mgc.core"), "app # project identity\n").unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();

    assert_eq!(detect_test_core(dir.path()).unwrap(), "app");
}

#[test]
fn invalid_core_marker_fails_closed_instead_of_falling_back_to_manifest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".mgc.core"), "not-a-core\n").unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();

    let error = detect_test_core(dir.path()).unwrap_err();
    assert!(
        error.to_string().contains("invalid core marker")
            && !error.to_string().contains("signature")
    );
}

#[cfg(unix)]
#[test]
fn symlinked_core_marker_is_rejected_by_test_runtime_detection() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside-marker");
    std::fs::write(&outside, "web\n").unwrap();
    std::os::unix::fs::symlink(&outside, dir.path().join(".mgc.core")).unwrap();

    assert!(detect_test_core(dir.path()).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_test_manifests_are_not_read_or_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("package.json"),
        r#"{"scripts":{"test":"echo external"}}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("package.json"),
        dir.path().join("package.json"),
    )
    .unwrap();

    assert!(detect_test_runner(dir.path()).is_err());

    std::fs::remove_file(dir.path().join("package.json")).unwrap();
    std::fs::write(
        outside.path().join("mgc.toml"),
        "[scripts]\ntest = \"echo outside\"\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path().join("mgc.toml"), dir.path().join("mgc.toml"))
        .unwrap();
    assert!(resolve_mgc_toml_script(&dir.path().join("mgc.toml"), "test").is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_swift_manifest_is_rejected_before_runner_selection() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("Package.swift"),
        "// swift-tools-version: 5.9\nlet package = Package(name: \"outside\")\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("Package.swift"),
        dir.path().join("Package.swift"),
    )
    .unwrap();

    assert!(detect_test_runner(dir.path()).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_legacy_manifest_is_rejected_for_test_core_detection() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("pyproject.toml"),
        "[project]\ndependencies = [\"torch\"]\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("pyproject.toml"),
        dir.path().join("pyproject.toml"),
    )
    .unwrap();
    assert!(detect_test_core(dir.path()).is_err());
}
