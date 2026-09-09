//! CLI audit E2E — binary contract for the unified audit pipeline
//! (Tech Lead 2026-09-09 P0-1/P0-6/P0-7).
//!
//! Proves the parsers wired in the adapters are reachable from the REAL
//! `mgc audit` binary — not just via direct Rust calls:
//! - lib (Rust): cargo-audit runs, RUSTSEC finding reported, exit 1.
//! - lib (Python): pip-audit runs through the same pipeline.
//! - app (Kotlin): missing gradle surfaces ToolMissing + remediation.
//! - exit 2 contract: scanner unavailable + MGC_AUDIT_STRICT=1 fails with
//!   the dedicated environment exit code (not the findings exit code).
//! - non-strict unavailable: exit 0 with a loud UNVERIFIED warning.
//!
//! E2E qua binary thật `mgc audit`: parser trong adapter phải chạy được
//! từ CLI (không chỉ gọi Rust trực tiếp) — finding RUSTSEC exit 1,
//! pip-audit Python, Kotlin ToolMissing kèm remediation, strict
//! unavailable exit 2, non-strict unavailable exit 0 kèm cảnh báo.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Resolve the cargo-built mgc binary — fresh for this run, .exe handled.
/// Lấy binary mgc cargo build cho run này — luôn tươi, tự xử lý .exe.
fn find_mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

/// `cargo-audit` installed? Hermetic guard — CI installs it (security.yml
/// pins 0.22.2); without it the vulnerable-fixture test must skip.
/// In a REQUIRED environment (CI sets MGC_E2E_AUDIT_TOOLS=required) a
/// missing tool FAILS the test instead of skipping (Tech Lead P0-5).
/// Hermetic guard: máy không có cargo-audit thì test finding phải bỏ qua.
/// Trong môi trường BẮT BUỘC (CI đặt MGC_E2E_AUDIT_TOOLS=required), thiếu
/// tool thì test FAIL thay vì skip.
fn cargo_audit_installed() -> bool {
    require_tools_or_fail("cargo-audit")
}

/// Real vulnerable fixture: rsa 0.9.10 is affected by RUSTSEC-2023-0071
/// (verified live 2026-09-09 with cargo-audit 0.22.2 — the same advisory
/// the parser fixture `cargo-audit-real-0.22.2.json` was captured from).
/// Fixture vulnerable THẬT: rsa 0.9.10 dính RUSTSEC-2023-0071.
fn vulnerable_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"vuln-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"rust\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"vuln-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         \n[dependencies]\nrsa = \"=0.9.10\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    // A real lockfile with the pinned vulnerable version — cargo-audit
    // reads the lockfile, so `cargo generate-lockfile` seeds it honestly.
    // Lockfile thật với version bị dính advisory — cargo-audit đọc lockfile.
    Command::new("cargo")
        .arg("generate-lockfile")
        .current_dir(dir)
        .output()
        .expect("cargo generate-lockfile failed");
}

/// Clean fixture: zero dependencies — cargo-audit reports zero findings.
/// Fixture sạch: không dependency — cargo-audit báo 0 finding.
fn clean_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"clean-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"rust\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"clean-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    Command::new("cargo")
        .arg("generate-lockfile")
        .current_dir(dir)
        .output()
        .expect("cargo generate-lockfile failed");
}

/// Run `mgc audit` (optionally with env) and return (exit_code, output).
/// Chạy `mgc audit` (kèm env tùy chọn), trả (exit_code, output).
fn run_mgc_audit(mgc: &str, cwd: &std::path::Path, strict: Option<bool>) -> (Option<i32>, String) {
    let mut cmd = Command::new(mgc);
    cmd.arg("audit").current_dir(cwd);
    if let Some(strict) = strict {
        cmd.env("MGC_AUDIT_STRICT", if strict { "1" } else { "0" });
    }
    let out = cmd.output().expect("failed to spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

/// Truly hermetic runner (Tech Lead P0-3 2026-09-09): the child PATH
/// contains ONLY a temp bin dir with the exact mgc binary — never the
/// host PATH — so `which gradle` (and any other tool probe) fails
/// deterministically on every machine, including machines with gradle
/// installed. Portable: join_paths handles the platform separator, and
/// on Windows the copied mgc exe keeps its name.
/// Runner hermetic thật sự: PATH của process con CHỈ chứa thư mục bin
/// tạm với đúng binary mgc — không bao giờ nối PATH gốc — nên `which
/// gradle` (và mọi tool probe khác) fail tất định trên mọi máy, kể cả
/// máy có cài gradle. Portable: join_paths xử lý dấu phân cách theo nền
/// tảng, trên Windows file exe giữ nguyên tên.
fn run_mgc_audit_no_gradle(
    mgc: &str,
    cwd: &std::path::Path,
    strict: Option<bool>,
) -> (Option<i32>, String) {
    let bin_dir = TempDir::new().unwrap();
    let mgc_path = std::path::Path::new(mgc);
    let dest = bin_dir
        .path()
        .join(mgc_path.file_name().unwrap_or_default());
    std::fs::copy(mgc_path, &dest).expect("copy mgc into hermetic bin dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .expect("chmod 755 mgc copy");
    }

    let mut cmd = Command::new(&dest);
    cmd.arg("audit").current_dir(cwd);
    // NO host PATH is joined — the hermetic dir is the ONLY entry.
    // KHÔNG nối PATH gốc — thư mục hermetic là entry DUY NHẤT.
    cmd.env(
        "PATH",
        std::env::join_paths([bin_dir.path()]).unwrap_or_default(),
    );
    if let Some(strict) = strict {
        cmd.env("MGC_AUDIT_STRICT", if strict { "1" } else { "0" });
    }
    let out = cmd.output().expect("failed to spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

#[test]
fn audit_lib_rust_via_cli_reports_real_rustsec_finding_exit_1() {
    if !cargo_audit_installed() {
        eprintln!("SKIP: cargo-audit not installed on this machine");
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_lib_project(sandbox.path());

    // The REAL binary must surface the parser through the whole pipeline:
    // detect → adapter → cargo-audit → typed parse → report → exit 1.
    // Binary THẬT phải đưa parser qua toàn pipeline: detect → adapter →
    // cargo-audit → parse typed → report → exit 1.
    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "findings must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("RUSTSEC-2023-0071"),
        "the real advisory id must appear in the CLI report:\n{output}"
    );
    assert!(
        output.contains("rsa"),
        "the affected package must appear in the CLI report:\n{output}"
    );
    assert!(
        !output.contains("not implemented"),
        "audit must no longer stub out with not-implemented:\n{output}"
    );
}

#[test]
fn audit_lib_rust_via_cli_clean_fixture_exit_0() {
    if !cargo_audit_installed() {
        eprintln!("SKIP: cargo-audit not installed on this machine");
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    clean_lib_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(code, Some(0), "clean fixture must exit 0:\n{output}");
    assert!(
        output.contains("No vulnerabilities"),
        "clean run must say so explicitly:\n{output}"
    );
}

/// pip-audit installed? Hermetic guard for the Python E2E below.
/// Same P0-5 contract: required environments fail instead of skipping.
/// Guard hermetic cho E2E Python — cùng hợp đồng P0-5: môi trường bắt
/// buộc thì fail thay vì skip.
fn pip_audit_installed() -> bool {
    require_tools_or_fail("pip-audit")
}

/// Shared guard: skip locally when the tool is absent (a missing local
/// tool proves nothing), but fail hard in required CI environments so
/// "passed" always means "executed" (Tech Lead P0-5: no silent skips in
/// release gates).
/// Guard chung: local thiếu tool thì skip (thiếu tool không chứng minh gì),
/// môi trường CI bắt buộc thì fail cứng — "passed" luôn nghĩa là "đã chạy".
fn require_tools_or_fail(tool: &str) -> bool {
    let installed = Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !installed {
        let required = std::env::var("MGC_E2E_AUDIT_TOOLS")
            .map(|v| v == "required")
            .unwrap_or(false);
        if required {
            panic!(
                "MGC_E2E_AUDIT_TOOLS=required but '{tool}' is not installed — the CI lane must provision it before running this test"
            );
        }
        eprintln!("SKIP (environment-unverified): {tool} not installed on this machine");
    }
    installed
}

/// Real vulnerable Python fixture: requests==2.19.0 is affected by
/// PYSEC-2018-28 / CVE-2018-18074 (verified live 2026-09-09 with
/// pip-audit 2.9.0 — exit 1 with the findings above).
/// Fixture Python dính lỗi thật: requests==2.19.0 dính PYSEC-2018-28 /
/// CVE-2018-18074 (đã chạy thật bằng pip-audit 2.9.0 — exit 1).
#[test]
fn audit_lib_python_via_cli_reports_real_vulnerability_exit_1() {
    if !pip_audit_installed() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"py-vuln\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("requirements.txt"),
        "requests==2.19.0\n",
    )
    .unwrap();

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "vulnerable python fixture must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("requests"),
        "the affected package must appear in the report:\n{output}"
    );
}

#[test]
fn audit_lib_python_via_cli_runs_real_pip_audit() {
    if !pip_audit_installed() {
        eprintln!("SKIP: pip-audit not installed on this machine");
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    // Python lib project with a PINNED dependency file — pip-audit must
    // audit THIS file (`-r requirements.txt`), not the ambient Python
    // environment (Tech Lead P0-3).
    // Project lib Python có file dependency ĐÃ GHIM — pip-audit phải audit
    // file NÀY (-r requirements.txt), không phải environment Python ngoài.
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"py-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("pyproject.toml"),
        "[project]\nname = \"py-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    // Empty pinned dep set: audits the file (0 deps) — provably the
    // project's own dependency set, and clean means clean.
    // Tập dep ghim rỗng: audit chính file (0 dep) — chứng minh là tập
    // dependency của project, sạch nghĩa là sạch thật.
    std::fs::write(
        sandbox.path().join("requirements.txt"),
        "# no dependencies\n",
    )
    .unwrap();

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(0),
        "clean python fixture must exit 0 via pip-audit, got {code:?}:\n{output}"
    );
    assert!(
        !output.contains("not implemented"),
        "python audit must run the real scanner, not a stub:\n{output}"
    );
    assert!(
        !output.contains("UNVERIFIED"),
        "pip-audit present means the audit is verified:\n{output}"
    );
}

/// P0-3 contract: unresolved pyproject.toml must NEVER fall back to
/// auditing the ambient environment — the scanner reports Failed with
/// real guidance instead.
/// Hợp đồng P0-3: pyproject.toml chưa resolve KHÔNG BAO GIỜ quay lại audit
/// environment ngoài — scanner trả Failed kèm hướng dẫn thật.
#[test]
fn audit_lib_python_unresolved_pyproject_fails_closed_not_environment() {
    if !pip_audit_installed() {
        eprintln!("SKIP: pip-audit not installed on this machine");
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"py-unresolved\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("pyproject.toml"),
        "[project]\nname = \"py-unresolved\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    // Deliberately NO requirements/lockfile.

    // Strict mode: the failed routing must surface as environment exit 2.
    // Strict: routing fail phải hiện thành exit 2 môi trường.
    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), Some(true));
    assert_eq!(
        code,
        Some(2),
        "unresolved python deps must exit 2 in strict, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("requirements.txt") || output.contains("requirements"),
        "guidance must tell the user how to resolve dependencies:\n{output}"
    );
    assert!(
        !output.contains("No vulnerabilities"),
        "an unresolved audit must never print a clean verdict:\n{output}"
    );
}

#[test]
fn audit_app_kotlin_without_gradle_reports_tool_missing_with_remediation() {
    // Hermetic PATH deterministically excludes gradle — this test runs
    // on EVERY machine (CI included), no host-dependent skip (P0-5).
    // PATH hermetic loại gradle tất định — test chạy trên MỌI máy (kể cả
    // CI), không skip theo máy dev.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    // Kotlin app project WITHOUT gradlew and WITHOUT gradle on the
    // hermetic PATH: the CLI must surface ToolMissing with install
    // guidance — never a silent clean. Strict mode fails with exit 2.
    // Project Kotlin không có gradlew/gradle trong PATH hermetic: CLI phải
    // ra ToolMissing kèm hướng dẫn cài — không được im lặng báo sạch.
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"kotlin-fixture\"\necosystem = \"app\"\n\n[app]\nlanguage = \"kotlin\"\n",
    )
    .unwrap();
    std::fs::write(sandbox.path().join("build.gradle"), "// empty\n").unwrap();

    let (code, output) = run_mgc_audit_no_gradle(&mgc, sandbox.path(), Some(true));
    assert_eq!(
        code,
        Some(2),
        "strict + ToolMissing must exit 2 (environment), got {code:?}:\n{output}"
    );
    assert!(
        output.contains("gradle") && output.contains("Remediation"),
        "ToolMissing must name the tool and remediation:\n{output}"
    );
}

#[test]
fn audit_unavailable_strict_fails_with_environment_exit_2() {
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    // App core with a pubspec → Flutter security audit is honestly
    // unavailable (no CVE scanner) → strict mode must fail with the
    // DEDICATED environment exit code, not the findings code.
    // Core app với pubspec → security audit Flutter trung thực
    // unavailable → strict phải fail với exit code môi trường riêng.
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"strict-unavailable\"\necosystem = \"app\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("pubspec.yaml"),
        "name: test\nenvironment:\n  sdk: ^3.0.0\n",
    )
    .unwrap();

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), Some(true));
    assert_eq!(
        code,
        Some(2),
        "strict + unavailable must exit 2 (environment), got {code:?}:\n{output}"
    );
    assert!(
        output.contains("UNVERIFIED") || output.contains("unavailable"),
        "the unverified state must be stated explicitly:\n{output}"
    );
}

#[test]
fn audit_unavailable_non_strict_exits_0_with_warning() {
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"non-strict-unavailable\"\necosystem = \"app\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("pubspec.yaml"),
        "name: test\nenvironment:\n  sdk: ^3.0.0\n",
    )
    .unwrap();

    // Local escape hatch: exit 0, but NEVER silent — the warning must be
    // loud (RULE §11: escape hatch luôn cảnh báo).
    // Escape hatch local: exit 0 nhưng KHÔNG im lặng — cảnh báo rõ.
    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), Some(false));
    assert_eq!(
        code,
        Some(0),
        "non-strict unavailable must exit 0 locally, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("UNVERIFIED"),
        "the unverified warning must be printed:\n{output}"
    );
}
