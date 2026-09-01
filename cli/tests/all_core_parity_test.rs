//! All-core parity test (Phase 2 — user 2026-08-31) — test đồng nhất 4 cores
//! All-core parity test: web/ai/app/lib must create projects equally.
//! Hermetic: temp HOME, no workspace templates/ — hermetic: HOME tạm, không dùng workspace templates/.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("mgc")
}

fn setup_hermetic_home() -> TempDir {
    let temp_home = TempDir::new().unwrap();
    std::env::set_var("HOME", temp_home.path());
    std::env::set_var("MGC_CACHE_DIR", temp_home.path().join(".mgc"));
    temp_home
}

#[test]
fn test_all_core_parity_embedded() {
    let _temp_home = setup_hermetic_home();
    let temp_workspace = TempDir::new().unwrap();
    let workspace_path = temp_workspace.path();

    // 1. Web vanilla (embedded kernel)
    let web_output = Command::new(mgc_binary())
        .args(&["create-web", "vanilla", "test-web", "--ts"])
        .current_dir(workspace_path)
        .output()
        .expect("Failed to execute mgc create-web");

    assert!(
        web_output.status.success(),
        "create-web vanilla failed: {}",
        String::from_utf8_lossy(&web_output.stderr)
    );
    assert!(
        workspace_path.join("test-web/index.html").exists(),
        "web/vanilla: index.html not created"
    );
    assert!(
        workspace_path.join("test-web/.mgc.core").exists(),
        "web: .mgc.core marker not created"
    );

    // 2. AI python-agent (embedded kernel)
    let ai_output = Command::new(mgc_binary())
        .args(&["create-ai", "python-agent", "test-ai"])
        .current_dir(workspace_path)
        .output()
        .expect("Failed to execute mgc create-ai");

    assert!(
        ai_output.status.success(),
        "create-ai python-agent failed: {}",
        String::from_utf8_lossy(&ai_output.stderr)
    );
    assert!(
        workspace_path.join("test-ai/pyproject.toml").exists(),
        "ai/python-agent: pyproject.toml not created"
    );
    assert!(
        workspace_path.join("test-ai/.mgc.core").exists(),
        "ai: .mgc.core marker not created"
    );

    // 3. App flutter (embedded kernel)
    let app_output = Command::new(mgc_binary())
        .args(&["create-app", "flutter@stable", "test-app"])
        .current_dir(workspace_path)
        .output()
        .expect("Failed to execute mgc create-app");

    assert!(
        app_output.status.success(),
        "create-app flutter failed: {}",
        String::from_utf8_lossy(&app_output.stderr)
    );
    assert!(
        workspace_path.join("test-app/pubspec.yaml").exists(),
        "app/flutter: pubspec.yaml not created"
    );
    assert!(
        workspace_path.join("test-app/.mgc.core").exists(),
        "app: .mgc.core marker not created"
    );

    // 4. Lib rust (embedded kernel)
    let lib_output = Command::new(mgc_binary())
        .args(&["create-lib", "rust@1.75.0", "test-lib"])
        .current_dir(workspace_path)
        .output()
        .expect("Failed to execute mgc create-lib");

    assert!(
        lib_output.status.success(),
        "create-lib rust failed: {}",
        String::from_utf8_lossy(&lib_output.stderr)
    );
    assert!(
        workspace_path.join("test-lib/Cargo.toml").exists(),
        "lib/rust: Cargo.toml not created"
    );
    assert!(
        workspace_path.join("test-lib/.mgc.core").exists(),
        "lib: .mgc.core marker not created"
    );

    // Verify all core markers
    let cores = ["web", "ai", "app", "lib"];
    for core in &cores {
        let marker_path = workspace_path.join(format!("test-{core}/.mgc.core"));
        let content = fs::read_to_string(marker_path).expect("Failed to read .mgc.core");
        assert_eq!(content.trim(), *core, "{core}: .mgc.core content mismatch");
    }
}

#[test]
fn test_all_core_cli_surface_uniform() {
    // Verify all cores have uniform CLI signature: <framework[@version]> <project>
    // (not testing execution, only clap definition)

    let mgc = mgc_binary();

    // Web: mgc create-web <FRAMEWORK[@VERSION]> <PROJECT>
    let web_help = Command::new(&mgc)
        .args(&["create-web", "--help"])
        .output()
        .expect("Failed to get create-web help");
    let web_help_str = String::from_utf8_lossy(&web_help.stdout);
    assert!(
        web_help_str.contains("FRAMEWORK[@VERSION]") && web_help_str.contains("PROJECT"),
        "create-web: CLI signature mismatch"
    );

    // AI: mgc create-ai <FRAMEWORK[@VERSION]> <PROJECT>
    let ai_help = Command::new(&mgc)
        .args(&["create-ai", "--help"])
        .output()
        .expect("Failed to get create-ai help");
    let ai_help_str = String::from_utf8_lossy(&ai_help.stdout);
    assert!(
        ai_help_str.contains("FRAMEWORK[@VERSION]") && ai_help_str.contains("PROJECT"),
        "create-ai: CLI signature mismatch"
    );

    // App: mgc create-app <FRAMEWORK[@VERSION]> <PROJECT>
    let app_help = Command::new(&mgc)
        .args(&["create-app", "--help"])
        .output()
        .expect("Failed to get create-app help");
    let app_help_str = String::from_utf8_lossy(&app_help.stdout);
    assert!(
        app_help_str.contains("FRAMEWORK[@VERSION]") && app_help_str.contains("PROJECT"),
        "create-app: CLI signature mismatch"
    );

    // Lib: mgc create-lib <FRAMEWORK[@VERSION]> <PROJECT> (Phase 2 fix — was just <PROJECT>)
    let lib_help = Command::new(&mgc)
        .args(&["create-lib", "--help"])
        .output()
        .expect("Failed to get create-lib help");
    let lib_help_str = String::from_utf8_lossy(&lib_help.stdout);
    assert!(
        lib_help_str.contains("FRAMEWORK[@VERSION]") && lib_help_str.contains("PROJECT"),
        "create-lib: CLI signature mismatch (should be <FRAMEWORK[@VERSION]> <PROJECT>)"
    );
}
