use super::*;

#[test]
fn update_args_empty_uv_locks_upgrade() {
    assert_eq!(update_args(&[], "uv"), vec!["lock", "--upgrade"]);
}

#[test]
fn update_args_empty_pip_lists_outdated() {
    assert_eq!(update_args(&[], "pip"), vec!["list", "--outdated"]);
}

#[test]
fn update_args_uv_upgrade_each_package() {
    let args = update_args(&["a b".to_string()], "uv");
    assert_eq!(
        args,
        vec![
            "lock",
            "--upgrade-package",
            "a",
            "--upgrade-package",
            "b"
        ]
    );
}

#[test]
fn update_args_pip_upgrade_packages() {
    let args = update_args(&["a".to_string(), "b".to_string()], "pip");
    assert_eq!(args, vec!["install", "--upgrade", "a", "b"]);
}