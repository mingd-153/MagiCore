use super::*;

#[test]
fn add_args_uv_uses_add() {
    let args = add_args(
        &["requests".to_string(), "uvicorn[standard]".to_string()],
        "uv",
        None,
    )
    .unwrap();
    assert_eq!(args, vec!["add", "requests", "uvicorn[standard]"]);
}

#[test]
fn add_args_pip_uses_install_and_splits_whitespace() {
    let args = add_args(&["a b".to_string(), "c".to_string()], "pip", None).unwrap();
    assert_eq!(args, vec!["install", "a", "b", "c"]);
}

#[test]
fn add_args_version_pins_with_pep508_equals() {
    // Verified semantics: `uv add` and `pip install` both accept
    // `name==version` (PEP 508); extras combine (`name[extra]==version`).
    let args = add_args(
        &["requests".to_string(), "uvicorn[standard]".to_string()],
        "uv",
        Some("2.31.0"),
    )
    .unwrap();
    assert_eq!(
        args,
        vec!["add", "requests==2.31.0", "uvicorn[standard]==2.31.0"]
    );
    let args = add_args(&["requests".to_string()], "pip", Some("2.31.0")).unwrap();
    assert_eq!(args, vec!["install", "requests==2.31.0"]);
}

#[test]
fn add_args_conflicting_spec_fails_loudly() {
    let err = add_args(&["requests>=2.0".to_string()], "pip", Some("2.31.0")).unwrap_err();
    assert!(err.to_string().contains("already carries"), "{err}");
    let err = add_args(&["pkg@1.0".to_string()], "uv", Some("1.0")).unwrap_err();
    assert!(err.to_string().contains("already carries"), "{err}");
}
