//! All-core parity test (Phase 2 — user 2026-08-31) — test đồng nhất 4 cores
//! All-core parity test: web/ai/app/lib must create projects equally.
//! Hermetic: temp HOME, no workspace templates/ — hermetic: HOME tạm, không dùng workspace templates/.

#![allow(clippy::unwrap_used)]
// Tests mutate env single-threaded (edition 2024 unsafe rule) — test đổi env 1 luồng.
#![allow(unsafe_code)]
#![allow(clippy::needless_borrows_for_generic_args)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> PathBuf {
    // Cargo-provided per-test binary: fresh for this run and portable across
    // platforms (adds .exe suffix on Windows automatically).
    // Binary do cargo cung cấp theo từng run test: tươi và portable mọi
    // nền tảng (tự thêm đuôi .exe trên Windows).
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

fn setup_hermetic_home() -> TempDir {
    let temp_home = TempDir::new().unwrap();
    unsafe { std::env::set_var("HOME", temp_home.path()) };
    unsafe { std::env::set_var("MGC_CACHE_DIR", temp_home.path().join(".mgc")) };
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

    // 4. Lib rust (embedded kernel) — use the moving `latest` tag, not a
    // pinned stale version: the scaffold emits edition 2024, which does
    // not compile on rust 1.75; version policy stays centralized.
    // Lib rust — dùng tag động `latest`, không pin version cũ: scaffold
    // sinh edition 2024 không compile trên rust 1.75; policy version tập
    // trung ở spec parser, không hardcode trong test.
    let lib_output = Command::new(mgc_binary())
        .args(&["create-lib", "rust@latest", "test-lib"])
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
