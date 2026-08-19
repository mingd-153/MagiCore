use super::*;

#[test]
fn add_args_uv_uses_add() {
    let args = add_args(&["requests".to_string(), "uvicorn[standard]".to_string()], "uv");
    assert_eq!(args, vec!["add", "requests", "uvicorn[standard]"]);
}

#[test]
fn add_args_pip_uses_install_and_splits_whitespace() {
    let args = add_args(&["a b".to_string(), "c".to_string()], "pip");
    assert_eq!(args, vec!["install", "a", "b", "c"]);
}