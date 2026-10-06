//! Bounded test subprocesses use the same native executor as the CLI harness.
//! Subprocess của test dùng cùng executor native có giới hạn với CLI harness.

mod common;

#[test]
fn executor_child_blocks() {
    if std::env::var_os("MGC_TEST_EXECUTOR_CHILD").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

#[test]
fn captured_test_process_timeout_is_bounded_and_not_success() {
    let executable = std::env::current_exe().expect("test executable");
    let options = mgc_exec::run::ExecOptions {
        timeout: Some(std::time::Duration::from_millis(150)),
        capture_full_stdout: true,
        allowed_exit_codes: (1..=255).collect(),
        env: vec![("MGC_TEST_EXECUTOR_CHILD".into(), "1".into())],
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let (success, diagnostic) = common::run_binary_with_options(
        &executable,
        &["--exact", "executor_child_blocks"],
        &options,
    );
    assert!(!success, "timed-out child must not be successful");
    assert!(diagnostic.contains("timed out"), "{diagnostic}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "process-tree termination must remain bounded"
    );
}
