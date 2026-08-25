#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-ai-adapter — sát với src/lib.rs
//! Kiểm thử: detect framework, adapter_for, PackageAdapter trait methods.

use mgc_ai_adapter::{adapter_for, detect_framework, AiAdapter, AiFramework};
use mgc_types::adapter::{AddOptions, PackageAdapter};
use mgc_types::PackageName;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-ai-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── detect_framework ───────────────────────────────────────────────────────

#[test]
fn detect_python_agent_via_pyproject_tool_magicore() {
    let dir = tmp("pa-pyp");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[tool.magicore]\nframework = \"python-agent\"\n",
    )
    .unwrap();
    assert_eq!(detect_framework(&dir), Some(AiFramework::PythonAgent));
}

#[test]
fn detect_mcp_server_via_pyproject_tool_magicore() {
    let dir = tmp("mcp-pyp");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[tool.magicore]\nframework = \"mcp-server\"\n",
    )
    .unwrap();
    assert_eq!(detect_framework(&dir), Some(AiFramework::McpServer));
}

#[test]
fn detect_python_agent_via_mgc_toml_ai_section() {
    let dir = tmp("pa-mgc");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    assert_eq!(detect_framework(&dir), Some(AiFramework::PythonAgent));
}

#[test]
fn detect_mcp_server_via_mgc_toml_ai_section() {
    let dir = tmp("mcp-mgc");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"mcp-server\"\n").unwrap();
    assert_eq!(detect_framework(&dir), Some(AiFramework::McpServer));
}

#[test]
fn detect_returns_none_with_no_marker_files() {
    let dir = tmp("empty");
    assert!(detect_framework(&dir).is_none());
}

#[test]
fn detect_returns_none_for_unknown_framework_value() {
    let dir = tmp("unknown");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[tool.magicore]\nframework = \"llm-wrapper\"\n",
    )
    .unwrap();
    // unknown framework → None
    assert!(detect_framework(&dir).is_none());
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_with_valid_marker() {
    let dir = tmp("af-ok");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"mcp-server\"\n").unwrap();
    assert!(adapter_for(&dir).is_some());
}

#[test]
fn adapter_for_returns_none_without_marker() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).is_none());
}

// ── AiFramework helpers ────────────────────────────────────────────────────

#[test]
fn aiframework_as_str_matches_scaffold_key() {
    assert_eq!(AiFramework::PythonAgent.as_str(), "python-agent");
    assert_eq!(AiFramework::McpServer.as_str(), "mcp-server");
}

#[test]
fn aiframework_entry_script_matches_scaffold() {
    assert_eq!(AiFramework::PythonAgent.entry_script(), "src/agent.py");
    assert_eq!(AiFramework::McpServer.entry_script(), "server.py");
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let adapter = AiAdapter {
        framework: AiFramework::PythonAgent,
    };
    assert_eq!(adapter.name(), "ai");
    assert_eq!(format!("{:?}", adapter.ecosystem()), "Ai");
}

#[test]
fn adapter_can_handle_returns_true_for_marked_project() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = AiAdapter {
        framework: AiFramework::PythonAgent,
    };
    assert!(adapter.can_handle(&dir));
}

#[test]
fn adapter_can_handle_returns_false_for_empty_dir() {
    let dir = tmp("ch-false");
    let adapter = AiAdapter {
        framework: AiFramework::PythonAgent,
    };
    assert!(!adapter.can_handle(&dir));
}

#[tokio::test]
async fn parse_manifest_uses_dir_name_as_project_name() {
    let dir = tmp("my-ai-project");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let manifest = adapter.parse_manifest(&dir).await.unwrap();
    assert!(manifest.name.contains("my-ai-project"));
}

#[tokio::test]
async fn resolve_returns_empty_graph_by_design() {
    let dir = tmp("resolve");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let manifest = adapter.parse_manifest(&dir).await.unwrap();
    let graph = adapter.resolve(&manifest).await.unwrap();
    // AI không quản lý deps — graph rỗng theo design
    assert!(graph.packages.is_empty());
}

#[tokio::test]
async fn install_fails_closed_with_descriptive_error() {
    let dir = tmp("install-fail");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let manifest = adapter.parse_manifest(&dir).await.unwrap();
    let graph = adapter.resolve(&manifest).await.unwrap();
    let result = adapter.install(&graph, &dir, Default::default()).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("pip"),
        "error message should mention pip: {msg}"
    );
}

#[tokio::test]
async fn add_fails_closed_with_descriptive_error() {
    let dir = tmp("add-fail");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let name = PackageName::new("openai").unwrap();
    let result = adapter.add(&dir, &name, None, AddOptions::default()).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("pip"),
        "error message should mention pip: {msg}"
    );
}

#[tokio::test]
async fn update_fails_closed_with_descriptive_error() {
    let dir = tmp("update-fail");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let name = PackageName::new("openai").unwrap();
    let result = adapter.update(&dir, Some(&name)).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn audit_returns_clean_report_for_empty_project() {
    let dir = tmp("audit");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"mcp-server\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let report = adapter.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}

#[tokio::test]
async fn list_returns_empty_for_project_with_no_deps() {
    let dir = tmp("list");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"mcp-server\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let pkgs = adapter.list(&dir).await.unwrap();
    assert!(pkgs.is_empty());
}
