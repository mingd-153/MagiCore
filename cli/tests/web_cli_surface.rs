// CLI surface tests: single-core vs multi-core commands.
// Multi-core build (all features enabled) → only create-web/add-web/remove-web/list-web exist.

mod common;

#[test]
fn test_help_shows_multi_core_commands() {
    common::assert_help_contains("create-web");
    common::assert_help_contains("add-web");
    common::assert_help_contains("remove-web");
    common::assert_help_contains("list-web");
    common::assert_help_contains("install-web");
}

#[test]
fn test_help_shows_core_option() {
    common::assert_help_contains("--core");
    common::assert_help_contains("web");
}

#[test]
fn test_create_web_accepts_flags() {
    let (ok, out) = common::mg(&["create-web", "--help"]);
    assert!(ok, "create-web --help failed\n{out}");
    assert!(out.contains("--dir"), "should mention --dir");
    assert!(out.contains("--ts"), "should mention --ts");
    assert!(out.contains("--js"), "should mention --js");
    assert!(out.contains("--git"), "should mention --git");
    assert!(out.contains("--install"), "should mention --install");
}

#[test]
fn test_install_web_accepts_flags() {
    let (ok, out) = common::mg(&["install-web", "--help"]);
    assert!(ok, "install-web --help failed\n{out}");
    assert!(out.contains("--frozen"), "should mention --frozen");
}

#[test]
fn test_add_web_accepts_flags() {
    let (ok, out) = common::mg(&["add-web", "--help"]);
    assert!(ok, "add-web --help failed\n{out}");
    assert!(out.contains("Add web dependencies"), "should mention plural dependencies");
    assert!(out.contains("--dev"), "should mention --dev");
    assert!(out.contains("--global"), "should mention --global");
}

#[test]
fn test_remove_web_accepts_multiple_packages() {
    let (ok, out) = common::mg(&["remove-web", "--help"]);
    assert!(ok, "remove-web --help failed\n{out}");
    assert!(out.contains("[PACKAGES]...") || out.contains("<PACKAGES>..."), "should mention repeated packages\n{out}");
    assert!(out.contains("--no-install"), "should mention --no-install");
}
