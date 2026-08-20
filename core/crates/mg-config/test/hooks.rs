#![allow(clippy::unwrap_used)]
//! Hooks tests (mg-config)

use mg_config::hooks::{list_hooks, run_hooks};

#[test]
fn hooks_run_and_fail() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mg.hooks.toml"),
        "hooks = { \"pre-install\" = [\"touch pre-ran.txt\", \"true\"] }\n",
    )
    .unwrap();
    run_hooks(dir.path(), "pre-install").unwrap();
    assert!(dir.path().join("pre-ran.txt").exists());
}

#[test]
fn hooks_failure_fails_command() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mg.hooks.toml"),
        "hooks = { \"post-publish\" = [\"false\"] }\n",
    )
    .unwrap();
    let err = run_hooks(dir.path(), "post-publish").unwrap_err();
    assert!(err.to_string().contains("failed"));
}

#[test]
fn hooks_list_events() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mg.hooks.toml"),
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
        dir.path().join("mg.hooks.toml"),
        "hooks = { \"pre-install\" = [\"echo ok && npm install\"] }\n",
    )
    .unwrap();
    let err = run_hooks(dir.path(), "pre-install").unwrap_err();
    assert!(err.to_string().contains("shell control operator"));
}

#[test]
fn hooks_reject_package_manager_tools() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mg.hooks.toml"),
        "hooks = { \"pre-install\" = [\"npm install\"] }\n",
    )
    .unwrap();
    let err = run_hooks(dir.path(), "pre-install").unwrap_err();
    assert!(err.to_string().contains("forbidden"));
}
