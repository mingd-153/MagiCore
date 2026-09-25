// Test-runner selection contracts — khóa lựa chọn runner theo ownership.
use super::detect_test_runner;

#[test]
fn flutter_test_disables_implicit_pub_resolution() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pubspec.yaml"), "name: fixture\n").unwrap();

    let (runner, args) = detect_test_runner(dir.path()).unwrap().unwrap();
    assert_eq!(runner, "flutter");
    assert_eq!(args, ["test", "--no-pub"]);
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
