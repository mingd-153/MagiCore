#![allow(clippy::unwrap_used)]
//! Allowlist tests — check tool cho phép/cấm/ngoài danh sách (00-index §5.1, §5.2)
//! (kiểm tra allowlist bất biến + forbidden npm family)

use mgc_exec::prelude::*;

#[test]
fn allows_known_tools() {
    for tool in [
        "cargo",
        "pip",
        "python",
        "python3",
        "go",
        "mvn",
        "composer",
        "node",
        "git",
        "docker",
        "flutter",
        "terraform",
    ] {
        check_tool(tool).unwrap_or_else(|e| panic!("{tool} should be allowed: {e}"));
    }
}

#[test]
fn rejects_forbidden_npm_family() {
    for tool in ["npm", "npx", "pnpm", "yarn", "bun", "bunx"] {
        let err = check_tool(tool).unwrap_err();
        assert!(
            err.to_string().contains("permanently forbidden"),
            "npm-family error must be explicit, got: {err}"
        );
    }
}

#[test]
fn rejects_forbidden_absolute_pm_paths() {
    for tool in [
        "/usr/bin/npm",
        "/opt/homebrew/bin/pnpm",
        "C:\\tools\\bun.exe",
    ] {
        let err = check_tool(tool).unwrap_err();
        assert!(
            err.to_string().contains("permanently forbidden"),
            "absolute PM path must be blocked, got: {err}"
        );
    }
}

#[test]
fn allows_absolute_paths_to_allowlisted_tools() {
    check_tool("/usr/bin/cargo").unwrap();
    check_tool("C:\\tools\\python.exe").unwrap();
}

#[test]
fn rejects_unknown_tool() {
    let err = check_tool("totally-made-up-tool").unwrap_err();
    assert!(err.to_string().contains("allowlist"));
}

#[test]
fn rejects_empty_name() {
    assert!(check_tool("").is_err());
    assert!(check_tool("   ").is_err());
}

#[test]
fn rejects_forbidden_pm_anywhere_in_script() {
    for script in [
        "vite --host localhost --port 4315 && npm install",
        "env PNPM_HOME=/tmp pnpm add zod",
        "node ./x.js; /usr/bin/yarn install",
        "\"bun\" install",
        "cmd /c npx vite",
        "bunx create-react-app demo",
    ] {
        let err = reject_forbidden_pm_script(script).unwrap_err();
        assert!(
            err.to_string().contains("forbidden package manager"),
            "unexpected error for {script}: {err}"
        );
    }
}

#[test]
fn allows_framework_local_scripts_without_pm_wrapper() {
    reject_forbidden_pm_script("vite --host localhost --port 4315").unwrap();
    reject_forbidden_pm_script("next dev --hostname localhost --port 4315").unwrap();
    reject_forbidden_pm_script("node ./node_modules/.bin/vite --host localhost").unwrap();
}

#[test]
fn parses_simple_script_without_shell() {
    let (program, args) =
        parse_simple_script(r#"node "./scripts/build app.js" --flag value"#).unwrap();
    assert_eq!(program, "node");
    assert_eq!(args, vec!["./scripts/build app.js", "--flag", "value"]);
}

#[test]
fn parses_leading_env_assignments_without_shell() {
    let invocation =
        parse_script_invocation(r#"NODE_ENV=production MGC_PORT=4315 vite --host localhost"#)
            .unwrap();

    assert_eq!(invocation.program, "vite");
    assert_eq!(invocation.args, vec!["--host", "localhost"]);
    assert_eq!(
        invocation.env,
        vec![
            ("NODE_ENV".to_string(), "production".to_string()),
            ("MGC_PORT".to_string(), "4315".to_string())
        ]
    );
}

#[test]
fn rejects_env_only_scripts() {
    let err = parse_script_invocation("NODE_ENV=production").unwrap_err();
    assert!(err.to_string().contains("missing a program"));
}

#[test]
fn rejects_shell_control_in_simple_script() {
    for script in [
        "node build.js && node post.js",
        "node build.js; rm -rf dist",
        "echo hi > out.txt",
        "node $(which build)",
        "node `which build`",
        "node build.js\nnode post.js",
    ] {
        let err = parse_simple_script(script).unwrap_err();
        assert!(
            err.to_string().contains("unsupported shell control"),
            "unexpected error for {script}: {err}"
        );
    }
}
