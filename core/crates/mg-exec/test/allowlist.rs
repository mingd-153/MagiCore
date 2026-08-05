//! Allowlist tests — check tool cho phép/cấm/ngoài danh sách (00-index §5.1, §5.2)
//! (kiểm tra allowlist bất biến + forbidden npm family)

use mg_exec::prelude::*;

#[test]
fn allows_known_tools() {
    for tool in ["cargo", "pip", "python", "docker", "flutter", "terraform"] {
        check_tool(tool).unwrap_or_else(|e| panic!("{tool} should be allowed: {e}"));
    }
}

#[test]
fn rejects_forbidden_npm_family() {
    for tool in ["npm", "npx", "pnpm", "yarn", "bun"] {
        let err = check_tool(tool).unwrap_err();
        assert!(
            err.to_string().contains("permanently forbidden"),
            "npm-family error must be explicit, got: {err}"
        );
    }
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