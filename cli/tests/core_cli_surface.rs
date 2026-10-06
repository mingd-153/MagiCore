#![allow(clippy::unwrap_used)]

// CLI surface tests: public commands must expose the four primary cores.
// Product rule: web/ai/app/lib stay visible together instead of single-core drift.

mod common;

#[test]
fn test_help_shows_multi_core_commands() {
    for core in ["web", "ai", "app", "lib"] {
        common::assert_help_contains(&format!("create-{core}"));
        common::assert_help_contains(&format!("add-{core}"));
        common::assert_help_contains(&format!("remove-{core}"));
        common::assert_help_contains(&format!("list-{core}"));
        common::assert_help_contains(&format!("install-{core}"));
    }
}

#[test]
fn test_help_shows_core_option() {
    common::assert_help_contains("--core");
    for core in ["web", "ai", "app", "lib"] {
        common::assert_help_contains(core);
    }
}

#[test]
fn test_create_web_accepts_flags() {
    let (ok, out) = common::mgc(&["create-web", "--help"]);
    assert!(ok, "create-web --help failed\n{out}");
    assert!(out.contains("--dir"), "should mention --dir");
    assert!(out.contains("--ts"), "should mention --ts");
    assert!(out.contains("--js"), "should mention --js");
    assert!(out.contains("--git"), "should mention --git");
    assert!(out.contains("--install"), "should mention --install");
}

#[test]
fn test_primary_core_create_aliases_are_executable() {
    for (alias, args) in [
        ("cre-w", vec!["cre-w", "--help"]),
        ("cre-ai", vec!["cre-ai", "--help"]),
        ("cre-a", vec!["cre-a", "--help"]),
        ("cre-l", vec!["cre-l", "--help"]),
    ] {
        let (ok, out) = common::mgc(&args);
        assert!(ok, "{alias} --help failed\n{out}");
    }
}

#[test]
fn test_primary_core_install_aliases_are_executable() {
    for (alias, args) in [
        ("i-web", vec!["i-web", "--help"]),
        ("i-ai", vec!["i-ai", "--help"]),
        ("i-app", vec!["i-app", "--help"]),
        ("i-lib", vec!["i-lib", "--help"]),
    ] {
        let (ok, out) = common::mgc(&args);
        assert!(ok, "{alias} --help failed\n{out}");
    }
}

#[test]
fn test_primary_non_web_create_commands_scaffold_real_projects() {
    let cases = [
        (
            vec!["create-ai", "python-agent", "ai-demo"],
            "ai-demo",
            "pyproject.toml",
            "ecosystem = \"ai\"",
        ),
        (
            vec!["create-app", "flutter", "app-demo"],
            "app-demo",
            "pubspec.yaml",
            "ecosystem = \"app\"",
        ),
        (
            vec!["create-lib", "rust", "lib-demo"],
            "lib-demo",
            "Cargo.toml",
            "ecosystem = \"lib\"",
        ),
    ];

    for (args, project, expected_file, mgc_toml_marker) in cases {
        let dir = common::work_dir();
        let (ok, out) = common::mgc_in(&dir, &args);
        assert!(ok, "{args:?} failed\n{out}");
        common::assert_file_exists(&dir.join(project), expected_file);
        common::assert_file_contains(&dir.join(project), "mgc.toml", mgc_toml_marker);
        common::assert_file_contains(&dir.join(project), ".mgc.core", project_core(project));
    }
}

fn project_core(project: &str) -> &'static str {
    match project {
        "ai-demo" => "ai",
        "app-demo" => "app",
        "lib-demo" => "lib",
        _ => unreachable!("test case must use a primary non-web project"),
    }
}

#[test]
fn test_install_accepts_dedupe_and_repair_flags() {
    let (ok, out) = common::mgc(&["install-web", "--help"]);
    assert!(ok, "install-web --help failed\n{out}");
    assert!(
        out.contains("--prefer-dedupe"),
        "should mention --prefer-dedupe"
    );
    assert!(out.contains("--repair"), "should mention --repair");
}

#[test]
fn test_store_prune_accepts_flags() {
    let (ok, out) = common::mgc(&["store", "prune", "--help"]);
    assert!(ok, "store prune --help failed\n{out}");
    assert!(out.contains("--dry-run"), "should mention --dry-run");
    assert!(out.contains("--json"), "should mention --json");
}

#[test]
fn test_install_web_accepts_flags() {
    let (ok, out) = common::mgc(&["install-web", "--help"]);
    assert!(ok, "install-web --help failed\n{out}");
    assert!(out.contains("--frozen"), "should mention --frozen");
}

#[test]
fn test_add_web_accepts_flags() {
    let (ok, out) = common::mgc(&["add-web", "--help"]);
    assert!(ok, "add-web --help failed\n{out}");
    assert!(
        out.contains("Add web dependencies"),
        "should mention plural dependencies"
    );
    assert!(out.contains("--dev"), "should mention --dev");
    assert!(out.contains("--global"), "should mention --global");
}

#[test]
fn test_bare_add_without_detected_core_fails_closed() {
    let dir = common::work_dir();
    let (ok, out) = common::mgc_in(&dir, &["add", "zod", "--no-install"]);
    assert!(!ok, "bare add should fail without core context\n{out}");
    assert!(
        out.contains("could not detect a MagiCore core"),
        "bare add should explain missing core context\n{out}"
    );
}

#[test]
fn test_bare_add_with_core_flag_keeps_single_core_path() {
    for core in ["web", "ai", "app", "lib"] {
        let (ok, out) = common::mgc(&["--core", core, "add", "--help"]);
        assert!(ok, "mgc --core {core} add --help failed\n{out}");
        assert!(out.contains("--no-install"), "should mention --no-install");
    }
}

#[test]
fn test_remove_web_accepts_multiple_packages() {
    let (ok, out) = common::mgc(&["remove-web", "--help"]);
    assert!(ok, "remove-web --help failed\n{out}");
    assert!(
        out.contains("[PACKAGES]...") || out.contains("<PACKAGES>..."),
        "should mention repeated packages\n{out}"
    );
    assert!(out.contains("--no-install"), "should mention --no-install");
}
