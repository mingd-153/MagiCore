#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn workspace_tool_writes_only_one_json_rpc_frame_to_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    )
    .unwrap();
    let package = tmp.path().join("packages/lib");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"@test/lib","version":"1.0.0"}"#,
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mgc"))
        .arg("mcp")
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"mgc_workspace_info","arguments":{}}}
"#,
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let frames: Vec<&str> = stdout.lines().collect();
    assert_eq!(frames.len(), 1, "unexpected stdout data: {stdout:?}");
    let response: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 7);
    let tool_text = response["result"]["content"][0]["text"].as_str().unwrap();
    let workspace: serde_json::Value = serde_json::from_str(tool_text).unwrap();
    assert_eq!(workspace["nodes"][0]["name"], "@test/lib");
}

#[test]
fn mcp_lists_extended_tools_and_redacts_config_resources() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("package.json"), r#"{"name":"root"}"#).unwrap();
    std::fs::write(
        tmp.path().join(".npmrc"),
        "registry=https://registry.npmjs.org/\n_authToken=local-secret-value\n",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mgc"))
        .arg("mcp")
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n\
              {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"resources/list\"}\n\
              {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"resources/read\",\"params\":{\"uri\":\"mgc://config/local\"}}\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let frames: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 3, "unexpected stdout data: {stdout:?}");

    let tool_names: Vec<&str> = frames[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for expected in [
        "mgc_install",
        "mgc_uninstall",
        "mgc_versions",
        "mgc_outdated",
        "mgc_config",
    ] {
        assert!(tool_names.contains(&expected), "missing tool {expected}");
    }

    let resource_uris: Vec<&str> = frames[1]["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|resource| resource["uri"].as_str())
        .collect();
    assert!(resource_uris.contains(&"mgc://workspace"));
    assert!(resource_uris.contains(&"mgc://capabilities"));
    assert!(resource_uris.contains(&"mgc://config/local"));

    let config_text = frames[2]["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(config_text.contains("***"));
    assert!(!config_text.contains("local-secret-value"));
}

#[test]
fn install_command_stdout_is_attached_to_its_mcp_result() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"empty-project","version":"1.0.0"}"#,
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mgc"))
        .arg("mcp")
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            br#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"mgc_install","arguments":{}}}
"#,
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let frames: Vec<&str> = stdout.lines().collect();
    assert_eq!(frames.len(), 1, "unexpected stdout data: {stdout:?}");
    let response: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
    assert_eq!(response["id"], 9);
    let tool_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(tool_text.contains("MagiCore install completed"));
    assert!(tool_text.contains("No dependencies to install."));
    assert!(tool_text.contains("Use 'mgc add <package>'"));
}
