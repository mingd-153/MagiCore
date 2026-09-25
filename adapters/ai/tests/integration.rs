#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-ai-adapter — sát với src/lib.rs
//! Kiểm thử: detect framework, adapter_for, PackageAdapter trait methods.

use mgc_ai_adapter::{AiAdapter, AiFramework, adapter_for, detect_framework, generate_sbom};
use mgc_types::adapter::{AddOptions, PackageAdapter};
use mgc_types::capabilities::{
    AuditProvider, ContentStoreProvider, CoreIdent, DependencyResolver, ProjectDetector,
};
use mgc_types::{PackageName, ResolvedGraph};
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-ai-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

/// Adapter WITHOUT the native PyPI lane (uv.lock/requirements-only shape).
/// (Adapter KHÔNG có lane PyPI native — hình dạng project uv.lock/requirements.)
fn adapter_without_lane() -> AiAdapter {
    AiAdapter {
        framework: AiFramework::PythonAgent,
        python_lane: None,
    }
}

#[test]
fn native_python_lane_does_not_take_over_foreign_lockfiles() {
    let dir = tmp("native-lane-lock-policy");
    std::fs::write(dir.join("pyproject.toml"), "[project]\nname='t'\n").unwrap();
    assert!(mgc_ai_adapter::uses_native_python_lane(&dir));
    std::fs::write(dir.join("uv.lock"), "version = 1\n").unwrap();
    assert!(!mgc_ai_adapter::uses_native_python_lane(&dir));
    std::fs::remove_file(dir.join("uv.lock")).unwrap();
    std::fs::write(dir.join("requirements.lock"), "six==1.17.0\n").unwrap();
    assert!(!mgc_ai_adapter::uses_native_python_lane(&dir));
}

#[test]
fn native_python_lane_rejects_unowned_lockfiles_and_dependency_sources() {
    let lockfiles = [
        "requirements.txt",
        "requirements-dev.txt",
        "pylock.toml",
        "pylock.production.toml",
        "poetry.lock",
        "pdm.lock",
        "Pipfile.lock",
        "pixi.lock",
        "conda-lock.yml",
    ];
    for (index, lockfile) in lockfiles.iter().enumerate() {
        let dir = tmp(&format!("foreign-python-lock-{index}"));
        std::fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname='t'\ndependencies=[]\n",
        )
        .unwrap();
        std::fs::write(dir.join(lockfile), "foreign owner\n").unwrap();
        assert!(
            !mgc_ai_adapter::uses_native_python_lane(&dir),
            "must not take ownership when {lockfile} exists"
        );
    }

    let poetry = tmp("poetry-project");
    std::fs::write(
        poetry.join("pyproject.toml"),
        "[tool.poetry]\nname='t'\n[tool.poetry.dependencies]\npython='^3.12'\nrequests='*'\n",
    )
    .unwrap();
    assert!(!mgc_ai_adapter::uses_native_python_lane(&poetry));

    let dynamic = tmp("dynamic-python-deps");
    std::fs::write(
        dynamic.join("pyproject.toml"),
        "[project]\nname='t'\ndynamic=['dependencies']\n",
    )
    .unwrap();
    assert!(!mgc_ai_adapter::uses_native_python_lane(&dynamic));
}

/// Adapter WITH the native PyPI lane (pyproject.toml project shape).
/// (Adapter CÓ lane PyPI native — hình dạng project pyproject.toml.)
fn adapter_with_lane(dir: &std::path::Path) -> AiAdapter {
    AiAdapter {
        framework: AiFramework::PythonAgent,
        python_lane: Some(
            mgc_lib_adapter::adapter_for_language(
                mgc_lib_adapter::LibLanguage::Python,
                dir,
                None,
                None,
            )
            .unwrap(),
        ),
    }
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
    let adapter = adapter_without_lane();
    assert_eq!(adapter.name(), "ai");
    assert_eq!(format!("{:?}", adapter.ecosystem()), "Ai");
}

#[test]
fn adapter_can_handle_returns_true_for_marked_project() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_with_lane(&dir);
    assert!(adapter.can_handle(&dir));
}

#[test]
fn adapter_can_handle_returns_false_for_empty_dir() {
    let dir = tmp("ch-false");
    let adapter = adapter_without_lane();
    assert!(!adapter.can_handle(&dir));
}

// ── PyPI lane (pyproject.toml) — capability claims per truth ───────────────

#[test]
fn non_lane_projects_do_not_claim_registry_capabilities() {
    let adapter = adapter_without_lane();
    let caps = adapter.capabilities();
    assert!(
        !caps.contains(&mgc_types::capabilities::Capability::DependencyResolver),
        "uv.lock/requirements-only projects keep registry caps unclaimed"
    );
    assert!(caps.contains(&mgc_types::capabilities::Capability::AuditProvider));
}

#[tokio::test]
async fn pypi_lane_claims_registry_capabilities_and_resolves_natively() {
    let dir = tmp("pypi-lane");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"t\"\n\n[tool.magicore]\nframework = \"python-agent\"\n",
    )
    .unwrap();
    let adapter = adapter_for(&dir).unwrap();
    assert!(
        adapter.python_lane.is_some(),
        "pyproject → native PyPI lane"
    );
    let caps = adapter.capabilities();
    assert!(caps.contains(&mgc_types::capabilities::Capability::DependencyResolver));
    assert!(caps.contains(&mgc_types::capabilities::Capability::LockfileProvider));
    assert!(caps.contains(&mgc_types::capabilities::Capability::ArtifactFetcher));
    assert!(caps.contains(&mgc_types::capabilities::Capability::ContentStoreProvider));
    let identity = mgc_types::adapter::PackageAdapter::manifest_identity(&adapter).unwrap();
    assert_eq!(identity.core, "ai");
    assert_eq!(identity.language, "python");
    // Empty manifest → native resolve returns an empty graph WITHOUT any
    // network call (no deps to resolve).
    // (Manifest rỗng → resolve native trả graph rỗng KHÔNG chạm network.)
    let manifest = adapter.parse_manifest(&dir).await.unwrap();
    let graph = adapter.resolve(&manifest).await.unwrap();
    assert!(graph.packages.is_empty());
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
async fn resolve_fails_closed_without_native_python_lane() {
    let dir = tmp("resolve");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let manifest = adapter.parse_manifest(&dir).await.unwrap();
    // AI does NOT claim DependencyResolver (Global Gate 1) — resolve must
    // now fail closed instead of returning an empty graph.
    // (AI KHÔNG claim DependencyResolver (Global Gate 1) — resolve phải
    // fail closed thay vì trả graph rỗng.)
    let result = adapter.resolve(&manifest).await;
    assert!(
        result.is_err(),
        "ai resolve must fail closed (no DependencyResolver claim)"
    );
}

#[tokio::test]
async fn install_fails_closed_with_descriptive_error() {
    let dir = tmp("install-fail");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let manifest = adapter.parse_manifest(&dir).await.unwrap();
    // resolve itself fails closed first (no DependencyResolver claim) —
    // install cannot even be reached without a graph.
    // (resolve đã fail closed trước (không claim DependencyResolver) —
    // install không thể gọi khi chưa có graph.)
    assert!(
        adapter.resolve(&manifest).await.is_err(),
        "ai resolve must fail closed"
    );
    let graph = ResolvedGraph::default();
    let result = adapter.install(&graph, &dir, Default::default()).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("ai core"),
        "error message should name the ai core: {msg}"
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
        msg.contains("ai core"),
        "error message should name the ai core: {msg}"
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
async fn remove_fails_closed_without_mutating_through_adapter_api() {
    let dir = tmp("remove-fail");
    let source =
        "[project]\nname = \"ai-test\"\nversion = \"0.1.0\"\ndependencies = [\"openai>=1.0\"]\n";
    std::fs::write(dir.join("pyproject.toml"), source).unwrap();
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"python-agent\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let name = PackageName::new("openai").unwrap();

    let error = adapter
        .remove(&dir, &name)
        .await
        .expect_err("direct adapter remove must fail closed outside CLI gateway");
    assert!(
        error
            .to_string()
            .contains("direct adapter mutation is disabled")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("pyproject.toml")).unwrap(),
        source
    );
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
async fn list_refuses_to_claim_an_mgc_owned_set_for_foreign_lane() {
    let dir = tmp("list");
    std::fs::write(dir.join("mgc.toml"), "[ai]\nframework = \"mcp-server\"\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let error = adapter.list(&dir).await.unwrap_err();
    assert!(error.to_string().contains("foreign Python lock/manifest"));
}

#[test]
fn generate_sbom_uses_lockfile_v2_fixture() {
    let mut lockfile = mgc_lockfile::Lockfile::new();
    lockfile.add_package(mgc_lockfile::Package::new(
        "transformers".to_string(),
        "4.35.0".to_string(),
        "https://pypi.org/transformers-4.35.0.tgz".to_string(),
        "blake3:ai123".to_string(),
    ));
    let json = generate_sbom(&lockfile, mgc_sbom::SbomOptions::default()).unwrap();
    assert!(json.contains("CycloneDX"));
    assert!(json.contains("transformers"));
    assert!(json.contains("4.35.0"));
}
