use super::*;

#[test]
fn list_args_uv_pipes_through_pip() {
    assert_eq!(list_args("uv"), vec!["pip", "list"]);
}

#[test]
fn list_args_pip_direct() {
    assert_eq!(list_args("pip"), vec!["list"]);
}