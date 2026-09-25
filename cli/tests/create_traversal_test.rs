//! Path traversal regression — CLI contract §5 security stress.
//! Regression test cho lỗ hổng create-* path traversal: binary thật phải
//! từ chối project name thoát cwd (../, absolute, drive prefix) và KHÔNG
//! tạo bất kỳ directory/file nào khi từ chối (fail atomically).

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

/// Resolve the cargo-built mgc binary — always fresh for this test run and
/// correct on every platform (.exe suffix handled by cargo).
/// Lấy binary mgc do cargo build cho run này — luôn tươi theo SHA test và
/// đúng mọi nền tảng (cargo tự xử lý đuôi .exe).
fn find_mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()?
                    .parent()
                    .map(|parent| parent.join("mgc"))
                    .map(|p| p.to_string_lossy().to_string())
            })
        })
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

fn run_create(mgc: &str, cwd: &std::path::Path, name: &str) -> (bool, String) {
    let out = Command::new(mgc)
        .arg("create-web")
        .arg("vanilla")
        .arg(name)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn mgc");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), format!("{stdout}{stderr}"))
}

#[test]
fn test_create_rejects_traversal_without_garbage() {
    let mgc = find_mgc_binary();

    for evil in [
        "../escape-attack",
        "/tmp/absolute-attack",
        "foo/../bar",
        "C:\\evil",
        "..",
    ] {
        let sandbox = TempDir::new().unwrap();
        let (ok, output) = run_create(&mgc, sandbox.path(), evil);
        assert!(
            !ok,
            "create-web with name {evil:?} must fail (got success)\n{output}"
        );
        assert!(
            output.contains("Invalid project name"),
            "actionable English error expected for {evil:?}, got:\n{output}"
        );
        // Fail atomically — nothing may be created inside the sandbox.
        // Fail atomically — không được tạo file nào trong sandbox.
        let leftovers: Vec<String> = std::fs::read_dir(sandbox.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            leftovers.is_empty(),
            "no garbage may be left for {evil:?}, found: {leftovers:?}"
        );
    }

    // REAL parent-escape probe: keep the parent alive, nest the sandbox one
    // level deeper, run "../name" from inside it, then verify the parent
    // gained no directory. This is the actual attack surface.
    // Test thoát cha THẬT: giữ parent sống, sandbox lồng sâu 1 cấp, chạy
    // "../name" từ trong đó rồi kiểm tra parent KHÔNG sinh directory nào.
    let parent = TempDir::new().unwrap();
    let sandbox = TempDir::new_in(parent.path()).unwrap();
    let (ok, output) = run_create(&mgc, sandbox.path(), "../escape-attack");
    assert!(!ok, "traversal name must fail, got success:\n{output}");
    assert!(
        !parent.path().join("escape-attack").exists(),
        "PARENT ESCAPE: directory created outside the sandbox cwd"
    );
    // The sandbox itself must also stay untouched (no partial residue).
    // Sandbox cũng phải sạch (không phần dư).
    let leftovers: Vec<String> = std::fs::read_dir(sandbox.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "sandbox must stay clean, found: {leftovers:?}"
    );
}

#[test]
fn test_create_rejects_separator_and_home_names() {
    let mgc = find_mgc_binary();
    for evil in ["a/b", "a\\b", "~root"] {
        let sandbox = TempDir::new().unwrap();
        let (ok, output) = run_create(&mgc, sandbox.path(), evil);
        assert!(!ok, "name {evil:?} must fail");
        assert!(
            output.contains("Invalid project name"),
            "expected invalid-project-name error for {evil:?}, got:\n{output}"
        );
        let leftovers: Vec<String> = std::fs::read_dir(sandbox.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(leftovers.is_empty(), "sandbox must stay clean for {evil:?}");
    }
}

#[test]
fn test_create_typo_tag_fails_early_with_suggestion() {
    // Contract from the release plan: `nextjs@laster` must fail early,
    // suggest `latest`, and leave no garbage directory.
    // Hợp đồng từ kế hoạch release: sai tag phải fail sớm + gợi ý, không rác.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    let out = Command::new(mgc)
        .arg("create-web")
        .arg("nextjs@laster")
        .arg("myTypoApp")
        .current_dir(sandbox.path())
        .output()
        .unwrap();
    assert!(!out.status.success(), "typo tag must fail");
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        text.contains("laster") && text.contains("latest"),
        "must suggest 'latest' for typo, got:\n{text}"
    );
    assert!(
        !sandbox.path().join("myTypoApp").exists(),
        "no garbage dir for typo tag"
    );
}
