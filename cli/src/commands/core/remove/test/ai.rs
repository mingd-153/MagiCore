use super::*;

#[test]
fn remove_args_uv_uses_remove() {
    let args = remove_args(&["flask".to_string()], "uv");
    assert_eq!(args, vec!["remove", "flask"]);
}

#[test]
fn remove_args_pip_uninstalls_yes() {
    let args = remove_args(&["a b".to_string()], "pip");
    assert_eq!(args, vec!["uninstall", "-y", "a", "b"]);
}