#![allow(clippy::unwrap_used)]
//! Hooks tests (mgc-config)

use mgc_config::hooks::{list_hooks, run_hooks};

#[test]
fn hooks_run_allowlisted_command() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"my-event\" = [\"cargo --version\"] }\n",
    )
    .unwrap();
    run_hooks(dir.path(), "my-event").unwrap();
}

#[test]
fn custom_hook_rejects_executable_outside_mgc_exec_policy() {
    let dir = tempfile::tempdir().unwrap();
    let harmless_but_unapproved = if cfg!(windows) {
        "cmd.exe /c exit 0"
    } else {
        "true"
    };
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        format!("hooks = {{ \"custom-event\" = [{harmless_but_unapproved:?}] }}\n"),
    )
    .unwrap();

    let error = run_hooks(dir.path(), "custom-event")
        .expect_err("custom hook must not bypass the shared process policy");
    assert!(
        error.to_string().contains("allowlist") || error.to_string().contains("forbidden"),
        "expected a policy refusal before execution, got: {error}"
    );
}

#[test]
fn dependency_lifecycle_hooks_reject_arbitrary_executables() {
    let dir = tempfile::tempdir().unwrap();
    let harmless_command = if cfg!(windows) {
        "cmd /c exit 0"
    } else {
        "true"
    };
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        format!("hooks = {{ \"pre-install\" = [{harmless_command:?}] }}\n"),
    )
    .unwrap();

    let err = run_hooks(dir.path(), "pre-install").unwrap_err();
    assert!(
        err.to_string().contains("disabled"),
        "arbitrary executable hooks must be refused before spawn: {err}"
    );
}

#[test]
fn hooks_failure_fails_command() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"my-event\" = [\"cargo --definitely-invalid-hook-argument\"] }\n",
    )
    .unwrap();
    let err = run_hooks(dir.path(), "my-event").unwrap_err();
    assert!(err.to_string().contains("failed"));
}

#[test]
fn hooks_list_events() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"pre-install\" = [\"echo hi\"] }\n",
    )
    .unwrap();
    let hooks = list_hooks(dir.path()).unwrap();
    assert!(hooks.contains_key("pre-install"));
}

#[test]
fn hooks_reject_shell_chaining() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"my-event\" = [\"echo ok && npm install\"] }\n",
    )
    .unwrap();
    let err = run_hooks(dir.path(), "my-event").unwrap_err();
    assert!(err.to_string().contains("shell control operator"));
}

#[test]
fn hooks_reject_package_manager_tools() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"pre-install\" = [\"npm install\"] }\n",
    )
    .unwrap();
    let err = run_hooks(dir.path(), "pre-install").unwrap_err();
    assert!(err.to_string().contains("disabled"));
}

#[test]
fn hooks_reject_toolchain_on_dependency_events() {
    // C0 firewall (T0.3/B4): cargo/uv/deno must not run on dependency
    // events even though they are not rival package managers.
    for (event, cmd) in [
        ("pre-install", "cargo fetch"),
        ("post-add", "uv sync"),
        ("pre-remove", "deno run x.ts"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mgc.hooks.toml"),
            format!("hooks = {{ \"{event}\" = [\"{cmd}\"] }}\n"),
        )
        .unwrap();
        let err = run_hooks(dir.path(), event).unwrap_err();
        assert!(
            err.to_string().contains("disabled"),
            "{event}/{cmd} must be refused: {err}"
        );
    }
}

#[test]
fn pre_dependency_hook_rejects_post_phase_before_operation_starts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"post-install\" = [\"python3 -c 'import subprocess; subprocess.run([\\\"npm\\\", \\\"install\\\"])'\"] }\n",
    )
    .unwrap();

    let err = run_hooks(dir.path(), "pre-install").unwrap_err();
    assert!(
        err.to_string().contains("disabled"),
        "post-phase wrapper must block before install mutates the project: {err}"
    );
}

#[test]
fn hooks_reject_package_managers_on_custom_events_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.hooks.toml"),
        "hooks = { \"my-event\" = [\"uv --version\"] }\n",
    )
    .unwrap();
    let error = run_hooks(dir.path(), "my-event")
        .expect_err("package managers must be rejected for every custom hook event");
    assert!(error.to_string().contains("forbidden"));
}
