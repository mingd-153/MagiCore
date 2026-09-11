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

/// Run `mgc audit --format json` (machine payload) and return
/// (exit_code, output).
/// Chạy `mgc audit --format json` (payload máy), trả (exit_code, output).
fn run_mgc_audit_json(
    mgc: &str,
    cwd: &std::path::Path,
    strict: Option<bool>,
) -> (Option<i32>, String) {
    let mut cmd = Command::new(mgc);
    cmd.args(["audit", "--format", "json"]).current_dir(cwd);
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
        // Marker carries the libtest thread name (= test name) so the
        // capability-matrix generator can downgrade THIS test's evidence
        // to unverified — a silent guard skip must never count as PASS.
        // Marker kèm tên thread libtest (= tên test) để bộ sinh
        // capability-matrix hạ cấp evidence của CHÍNH test này — guard
        // skip im lặng không bao giờ được tính là PASS.
        let test_name = std::thread::current()
            .name()
            .unwrap_or("unknown-test")
            .to_string();
        eprintln!(
            "SKIP (environment-unverified) test={test_name}: {tool} not installed on this machine"
        );
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

// ---------------------------------------------------------------------------
// Go lane E2E (P1 matrix row "Lib Go govulncheck") — real binary, real
// scanner, real vulnerable fixture.
// Lane E2E Go — binary thật, scanner thật, fixture dính lỗi thật.
// ---------------------------------------------------------------------------

/// govulncheck installed? Same P0-5 guard contract as the other tools.
/// Có govulncheck? Cùng hợp đồng guard P0-5 như các tool khác.
fn govulncheck_installed() -> bool {
    require_tools_or_fail("govulncheck")
}

/// Real vulnerable Go fixture: golang.org/x/text v0.3.2 is affected by
/// GO-2020-0015 / CVE-2020-14040 (verified live 2026-09-09 with
/// govulncheck v1.8.0).
/// Fixture Go dính lỗi thật: golang.org/x/text v0.3.2 dính GO-2020-0015 /
/// CVE-2020-14040 (đã chạy thật bằng govulncheck v1.8.0).
fn vulnerable_go_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"go-vuln-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"go\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("go.mod"),
        concat!(
            "module go-vuln-fixture\n\n",
            "go 1.26\n\n",
            "require golang.org/x/text v0.3.2\n",
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.go"),
        concat!(
            "package main\n\n",
            "import (\n",
            "\t\"fmt\"\n",
            "\t\"golang.org/x/text/encoding/charmap\"\n",
            ")\n\n",
            "func main() {\n",
            "\tfmt.Println(charmap.Windows1252.String())\n",
            "}\n",
        ),
    )
    .unwrap();
    // govulncheck needs a resolved module graph: `go mod tidy` writes
    // go.sum so the scan can verify module hashes (verified live
    // 2026-09-09; without go.sum the tool errors before scanning).
    // govulncheck cần đồ thị module đã resolve: `go mod tidy` ghi go.sum
    // để scan kiểm chứng hash module (đã chạy thật; thiếu go.sum tool
    // lỗi trước khi kịp quét).
    Command::new("go")
        .args(["mod", "tidy"])
        .current_dir(dir)
        .output()
        .expect("go mod tidy failed");
}

#[test]
fn audit_lib_go_via_cli_reports_real_osv_finding_exit_1() {
    if !govulncheck_installed() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_go_project(sandbox.path());

    // Resolve the module offline-able fixture first: govulncheck needs
    // the module in the local cache; `go mod download` warms it.
    // Resolve fixture trước: govulncheck cần module trong cache local;
    // `go mod download` làm ấm.
    let download = Command::new("go")
        .args(["mod", "download"])
        .current_dir(sandbox.path())
        .output();
    if let Ok(out) = &download
        && !out.status.success()
    {
        eprintln!(
            "SKIP (environment-unverified) test=audit_lib_go_via_cli_reports_real_osv_finding_exit_1: go mod download failed (offline?): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "vulnerable go fixture must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("GO-2020-0015") || output.contains("CVE-2020-14040"),
        "the real advisory must appear in the report:\n{output}"
    );
    assert!(
        output.contains("text"),
        "the affected module must appear in the report:\n{output}"
    );
}

#[test]
fn audit_lib_go_via_cli_clean_fixture_exit_0() {
    if !govulncheck_installed() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"go-clean-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"go\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("go.mod"),
        "module go-clean-fixture\n\ngo 1.26\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("main.go"),
        "package main\n\nimport \"fmt\"\n\nfunc main() { fmt.Println(\"ok\") }\n",
    )
    .unwrap();

    // CLEAN means: no DEPENDENCY findings. The machine's own Go stdlib
    // may carry live advisories (e.g. GO-2026-* for an unpatched local
    // toolchain) — those are environment findings, not fixture findings,
    // so the assertion checks the module level via the JSON payload.
    // SẠCH nghĩa là: KHÔNG có finding DEPENDENCY. Stdlib Go của máy có
    // thể mang advisory sống (toolchain local chưa patch) — đó là
    // finding môi trường chứ không phải fixture, nên assert ở mức module
    // qua payload JSON.
    let (code, output) = run_mgc_audit_json(&mgc, sandbox.path(), None);
    assert!(
        output.contains("\"scanner_status\": \"available\""),
        "govulncheck must complete (exit {code:?}):\n{output}"
    );
    assert!(
        !output.contains("golang.org/x/text"),
        "the clean fixture must not report the vulnerable module:\n{output}"
    );
    assert!(
        !output.contains("UNVERIFIED"),
        "govulncheck present means the audit is verified:\n{output}"
    );
    // Zero-dependency fixture: exit is driven ONLY by any stdlib
    // advisories on this machine — either exit code proves the scan ran.
    // Fixture không dependency: exit chỉ do advisory stdlib của máy —
    // cả hai exit code đều chứng minh scan đã chạy.
}

#[test]
fn audit_lib_go_without_govulncheck_reports_tool_missing() {
    // Hermetic PATH (no govulncheck) + strict → ToolMissing exit 2.
    // PATH hermetic (không govulncheck) + strict → ToolMissing exit 2.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"go-no-tool\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"go\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("go.mod"),
        "module go-no-tool\n\ngo 1.26\n",
    )
    .unwrap();

    let (code, output) = run_mgc_audit_no_gradle(&mgc, sandbox.path(), Some(true));
    assert_eq!(
        code,
        Some(2),
        "strict + govulncheck missing must exit 2 (environment), got {code:?}:\n{output}"
    );
    assert!(
        output.contains("govulncheck") && output.contains("Remediation"),
        "ToolMissing must name the tool and remediation:\n{output}"
    );
}

#[test]
fn audit_lib_rust_without_cargo_audit_reports_tool_missing() {
    // Hermetic PATH (no cargo-audit) + strict → ToolMissing exit 2 with
    // remediation. NOTE: the scanner probes `cargo-audit` via `which`,
    // but cargo itself must be absent too — the hermetic dir contains
    // ONLY the mgc binary, so both probes fail deterministically.
    // PATH hermetic (không cargo-audit) + strict → ToolMissing exit 2 kèm
    // remediation. Scanner dò `cargo-audit` bằng `which`; cargo cũng phải
    // vắng mặt — thư mục hermetic chỉ chứa binary mgc, nên mọi probe fail
    // tất định.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"rust-no-tool\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"rust\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("Cargo.toml"),
        "[package]\nname = \"rust-no-tool\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(sandbox.path().join("src")).unwrap();
    std::fs::write(sandbox.path().join("src/lib.rs"), "pub fn f() {}\n").unwrap();

    let (code, output) = run_mgc_audit_no_gradle(&mgc, sandbox.path(), Some(true));
    assert_eq!(
        code,
        Some(2),
        "strict + cargo-audit missing must exit 2 (environment), got {code:?}:\n{output}"
    );
    assert!(
        output.contains("cargo-audit") && output.contains("Remediation"),
        "ToolMissing must name the tool and remediation:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// P2 2026-09-10 — Java/.NET (OSV Maven/NuGet) + Swift (OSV SwiftURL) +
// Dart (OSV Pub) E2E lanes: real binary, REAL network advisories.
// Fixtures verified live 2026-09-10: commons-text 1.9 → GHSA-599f
// (Text4Shell); Newtonsoft.Json 12.0.2 → GHSA-5crp; http 0.13.0 (Pub)
// → GHSA-4rgh; swift-nio-http2 1.40.0 → GHSA-q3g2 + GHSA-4px2 (with
// the FIXED github.com/owner/repo naming — the old URL-git naming
// queried EMPTY, a fake clean).
// Lane E2E Java/.NET/Swift/Dart qua OSV thật. Fixture đã verify sống.
// ---------------------------------------------------------------------------

/// OSV reachable? These lanes hit the LIVE OSV API; without network the
/// tests skip honestly (a network-less run proves nothing). In required
/// CI environments (MGC_E2E_AUDIT_TOOLS=required) network is assumed.
/// OSV có truy cập được không? Lane này gọi OSV sống; không có mạng thì
/// skip trung thực. Môi trường CI bắt buộc coi network là có sẵn.
fn osv_reachable() -> bool {
    // Bounded connect (5s): an UNBOUNDED connect blocks the thread when
    // DNS/network is slow — and under a busy test machine that made the
    // OSV lanes skip spuriously. A short deadline keeps the guard honest
    // without hanging the suite.
    // Connect có giới hạn (5s): connect KHÔNG giới hạn sẽ treo thread khi
    // DNS/mạng chậm — máy test bận từng khiến lane OSV skip giả. Deadline
    // ngắn giữ guard trung thực mà không treo suite.
    use std::net::ToSocketAddrs;
    let addr: std::net::SocketAddr = match "api.osv.dev:443".to_socket_addrs() {
        Ok(mut addrs) => addrs.next().expect("api.osv.dev resolves"),
        Err(err) => panic!("api.osv.dev DNS resolution failed: {err}"),
    };
    match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(5)) {
        Ok(_) => true,
        Err(err) => {
            let required = std::env::var("MGC_E2E_AUDIT_TOOLS")
                .map(|v| v == "required")
                .unwrap_or(false);
            if required {
                panic!("MGC_E2E_AUDIT_TOOLS=required but api.osv.dev is unreachable: {err}");
            }
            let test_name = std::thread::current()
                .name()
                .unwrap_or("unknown-test")
                .to_string();
            eprintln!(
                "SKIP (environment-unverified) test={test_name}: api.osv.dev unreachable (offline?)"
            );
            false
        }
    }
}

fn vulnerable_java_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"java-vuln-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"java\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("gradle")).unwrap();
    std::fs::write(
        dir.join("gradle").join("verification-metadata.xml"),
        concat!(
            "<components>\n",
            "  <dependency group=\"org.apache.commons\" name=\"commons-text\" version=\"1.9\">\n",
            "  </dependency>\n",
            "</components>\n",
        ),
    )
    .unwrap();
}

fn clean_java_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"java-clean-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"java\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("gradle")).unwrap();
    std::fs::write(
        dir.join("gradle").join("verification-metadata.xml"),
        concat!(
            "<components>\n",
            "  <dependency group=\"com.google.guava\" name=\"guava\" version=\"32.0.0-jre\">\n",
            "  </dependency>\n",
            "</components>\n",
        ),
    )
    .unwrap();
}

#[test]
fn audit_lib_java_via_cli_reports_real_osv_finding_exit_1() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_java_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "vulnerable java fixture must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("GHSA-599f") || output.contains("CVE-2022-42889"),
        "the Text4Shell advisory must appear:\n{output}"
    );
    assert!(
        output.contains("commons-text"),
        "the affected package must appear:\n{output}"
    );
}

#[test]
fn audit_lib_java_via_cli_clean_fixture_exit_0() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    clean_java_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(code, Some(0), "clean fixture must exit 0:\n{output}");
    assert!(
        output.contains("No vulnerabilities"),
        "clean run must say so explicitly:\n{output}"
    );
}

fn vulnerable_dotnet_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"dotnet-vuln-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"dotnet\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages.lock.json"),
        concat!(
            "{\n",
            "  \"version\": 1,\n",
            "  \"dependencies\": {\n",
            "    \"net8.0\": {\n",
            "      \"dependencies\": {\n",
            "        \"Newtonsoft.Json\": {\"type\": \"Direct\", \"version\": \"12.0.2\"}\n",
            "      }\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    )
    .unwrap();
}

fn clean_dotnet_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"dotnet-clean-fixture\"\necosystem = \"lib\"\n\n[lib]\nlanguage = \"dotnet\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages.lock.json"),
        concat!(
            "{\n",
            "  \"version\": 1,\n",
            "  \"dependencies\": {\n",
            "    \"net8.0\": {\n",
            "      \"dependencies\": {\n",
            "        \"Serilog\": {\"type\": \"Direct\", \"version\": \"3.1.1\"}\n",
            "      }\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    )
    .unwrap();
}

#[test]
fn audit_lib_dotnet_via_cli_reports_real_osv_finding_exit_1() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_dotnet_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "vulnerable dotnet fixture must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("GHSA-5crp"),
        "the real advisory must appear:\n{output}"
    );
    assert!(
        output.contains("Newtonsoft.Json"),
        "the affected package must appear:\n{output}"
    );
}

#[test]
fn audit_lib_dotnet_via_cli_clean_fixture_exit_0() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    clean_dotnet_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(code, Some(0), "clean fixture must exit 0:\n{output}");
    assert!(
        output.contains("No vulnerabilities"),
        "clean run must say so explicitly:\n{output}"
    );
}

fn vulnerable_swift_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"swift-vuln-fixture\"\necosystem = \"app\"\n\n[app]\nlanguage = \"swift\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Package.resolved"),
        concat!(
            "{\n",
            "  \"pins\": [\n",
            "    {\"identity\": \"swift-nio-http2\", \"location\": \"https://github.com/apple/swift-nio-http2.git\", \"state\": {\"version\": \"1.40.0\"}}\n",
            "  ],\n",
            "  \"version\": 2\n",
            "}\n",
        ),
    )
    .unwrap();
}

#[test]
fn audit_app_swift_via_cli_reports_real_osv_findings_exit_1() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_swift_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "vulnerable swift fixture must exit 1, got {code:?}:\n{output}"
    );
    // The naming fix: github.com/apple/swift-nio-http2 (not the raw git
    // URL) is what OSV matches — both advisories must surface.
    // Sửa tên: github.com/apple/swift-nio-http2 (không phải URL git gốc)
    // mới khớp OSV — cả hai advisory phải hiện.
    assert!(
        output.contains("GHSA-q3g2") || output.contains("GHSA-4px2"),
        "a real swift advisory must appear:\n{output}"
    );
    assert!(
        output.contains("swift-nio-http2"),
        "the affected package must appear:\n{output}"
    );
}

fn vulnerable_flutter_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"flutter-vuln-fixture\"\necosystem = \"app\"\n\n[app]\nlanguage = \"flutter\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pubspec.lock"),
        concat!(
            "packages:\n",
            "  http:\n",
            "    version: \"0.13.0\"\n",
            "    source: hosted\n",
            "  args:\n",
            "    version: \"2.4.0\"\n",
            "    source: hosted\n",
        ),
    )
    .unwrap();
}

#[test]
fn audit_app_flutter_via_cli_reports_real_osv_finding_exit_1() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_flutter_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(1),
        "vulnerable flutter fixture must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("GHSA-4rgh"),
        "the real http advisory must appear:\n{output}"
    );
    assert!(
        output.contains("http"),
        "the affected package must appear:\n{output}"
    );
}

fn clean_swift_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"swift-clean-fixture\"\necosystem = \"app\"\n\n[app]\nlanguage = \"swift\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Package.resolved"),
        concat!(
            "{\n",
            "  \"pins\": [\n",
            "    {\"identity\": \"swift-argument-parser\", \"location\": \"https://github.com/apple/swift-argument-parser.git\", \"state\": {\"version\": \"1.5.0\"}}\n",
            "  ],\n",
            "  \"version\": 2\n",
            "}\n",
        ),
    )
    .unwrap();
}

#[test]
fn audit_app_swift_via_cli_clean_fixture_exit_0() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    clean_swift_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(code, Some(0), "clean swift fixture must exit 0:\n{output}");
    assert!(
        output.contains("No vulnerabilities"),
        "clean run must say so explicitly:\n{output}"
    );
}

fn clean_flutter_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"flutter-clean-fixture\"\necosystem = \"app\"\n\n[app]\nlanguage = \"flutter\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pubspec.lock"),
        concat!(
            "packages:\n",
            "  args:\n",
            "    version: \"2.4.0\"\n",
            "    source: hosted\n",
        ),
    )
    .unwrap();
}

#[test]
fn audit_app_flutter_via_cli_clean_fixture_exit_0() {
    if !osv_reachable() {
        return;
    }
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    clean_flutter_project(sandbox.path());

    let (code, output) = run_mgc_audit(&mgc, sandbox.path(), None);
    assert_eq!(
        code,
        Some(0),
        "clean flutter fixture must exit 0:\n{output}"
    );
    assert!(
        output.contains("No vulnerabilities"),
        "clean run must say so explicitly:\n{output}"
    );
}

/// Run `mgc audit` with a DEAD OSV endpoint (deterministic tool-failure
/// lane): the scanner must fail honestly, and strict mode must turn it
/// into exit 2 — never a fake clean.
/// Chạy `mgc audit` với endpoint OSV CHẾT (lane tool-failure tất định):
/// scanner phải fail trung thực, strict mode chuyển thành exit 2 — không
/// bao giờ sạch giả.
#[test]
fn audit_osv_unreachable_strict_fails_with_environment_exit_2() {
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_flutter_project(sandbox.path());

    let out = Command::new(&mgc)
        .arg("audit")
        .current_dir(sandbox.path())
        .env("MGC_OSV_API_BASE", "http://127.0.0.1:1") // port 1: refused instantly
        .env("MGC_AUDIT_STRICT", "1")
        .output()
        .expect("spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "strict + dead OSV must exit 2 (environment), got {:?}:\n{text}",
        out.status.code()
    );
    assert!(
        text.contains("UNVERIFIED") || text.contains("osv"),
        "the failure must name the scanner state:\n{text}"
    );
}

#[test]
fn audit_osv_unreachable_non_strict_exits_0_with_warning() {
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_dotnet_project(sandbox.path());

    let out = Command::new(&mgc)
        .arg("audit")
        .current_dir(sandbox.path())
        .env("MGC_OSV_API_BASE", "http://127.0.0.1:1")
        .env("MGC_AUDIT_STRICT", "0")
        .output()
        .expect("spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "non-strict + dead OSV is the escape hatch (exit 0 + warning):\n{text}"
    );
    assert!(
        text.contains("UNVERIFIED"),
        "the unverified state must be loud:\n{text}"
    );
}

#[test]
fn audit_osv_unreachable_java_strict_exit_2() {
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_java_project(sandbox.path());
    let out = Command::new(&mgc)
        .arg("audit")
        .current_dir(sandbox.path())
        .env("MGC_OSV_API_BASE", "http://127.0.0.1:1")
        .env("MGC_AUDIT_STRICT", "1")
        .output()
        .expect("spawn mgc");
    assert_eq!(
        out.status.code(),
        Some(2),
        "strict + dead OSV (java) must exit 2:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn audit_osv_unreachable_swift_strict_exit_2() {
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    vulnerable_swift_project(sandbox.path());
    let out = Command::new(&mgc)
        .arg("audit")
        .current_dir(sandbox.path())
        .env("MGC_OSV_API_BASE", "http://127.0.0.1:1")
        .env("MGC_AUDIT_STRICT", "1")
        .output()
        .expect("spawn mgc");
    assert_eq!(
        out.status.code(),
        Some(2),
        "strict + dead OSV (swift) must exit 2:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------------------
// Bun/Deno/npm lockfile binary E2E (P2 2026-09-10): the web advisory flow
// runs against a LOCAL mock registry (loopback is an explicitly allowed
// registry override) — deterministic, no internet, CI-provable.
// E2E binary lockfile Bun/Deno/npm: đường advisory web chạy với mock
// registry LOCAL (loopback được cho phép override) — tất định, không cần
// internet, chạy được trong CI.
// ---------------------------------------------------------------------------

/// A one-shot mock npm registry serving the bulk advisory endpoint with
/// ONE advisory: lodash <4.17.21 (GHSA-36p3-pj4w-937p XSS — the same
/// advisory shape npmjs returns; verified against the typed schema).
/// Mock registry một lần: endpoint bulk advisory trả MỘT advisory
/// lodash <4.17.21 (đúng shape npm Bulk API).
fn spawn_mock_npm_registry() -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let handle = std::thread::spawn(move || {
        // Serve MULTIPLE sequential connections (a client may reconnect);
        // the listener errors out once the test drops it — that ends the
        // loop. One request per connection, Connection: close.
        // Phục vụ NHIỀU connection tuần tự (client có thể reconnect);
        // listener lỗi khi test drop nó — đó là điểm dừng. Mỗi connection
        // một request, Connection: close.
        while let Ok((stream, _)) = listener.accept() {
            serve_bulk_advisory(stream);
        }
    });
    (addr, handle)
}

fn serve_bulk_advisory(mut stream: std::net::TcpStream) {
    use std::io::{Read, Write};
    // Drain the request head (we don't need the body for the mock).
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);
    let body = concat!(
        r#"{"lodash": [{"id": 1004279, "url": "https://github.com/advisories/GHSA-36p3-pj4w-937p","#,
        r#""title": "Command injection in lodash", "severity": "high","#,
        r#""vulnerable_versions": "<4.17.21"}]}"#
    );
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .as_bytes(),
    );
}

/// Run `mgc audit` with the mock registry override.
/// Chạy `mgc audit` với override registry mock.
fn run_mgc_audit_mock_registry(
    mgc: &str,
    cwd: &std::path::Path,
    addr: &std::net::SocketAddr,
) -> (Option<i32>, String) {
    let out = Command::new(mgc)
        .arg("audit")
        .current_dir(cwd)
        .env("MAGICORE_WEB_REGISTRY_URL", format!("http://{addr}"))
        .output()
        .expect("spawn mgc");
    (
        out.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn audit_web_bun_lock_via_cli_reports_mock_advisory_exit_1() {
    // P0-3 CONTRACT (2026-09-10): bun.lock là INPUT MIGRATION — audit
    // tiêu thụ mgc.lock. Flow E2E: bun.lock → `mgc import` → mgc.lock →
    // audit exit 1 với vulnerable pin. Audit trực tiếp bun.lock (không
    // import) phải bị từ chối (xem test fail-closed riêng).
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"bun-fixture\"\necosystem = \"web\"\n",
    )
    .unwrap();
    // Real bun.lock writer shape (captured 2026-09-10): JSONC with
    // trailing commas; lodash 4.17.20 < 4.17.21 is vulnerable.
    std::fs::write(
        sandbox.path().join("bun.lock"),
        r#"{
  "lockfileVersion": 1,
  "packages": {
    "lodash": ["lodash@4.17.20", "", {}, "sha512-Plhd"],
  }
}"#,
    )
    .unwrap();

    // Migration step: bun.lock → mgc.lock (import phải thành công).
    let import_out = Command::new(&mgc)
        .arg("import")
        .current_dir(sandbox.path())
        .output()
        .expect("spawn mgc import");
    let import_output = format!(
        "{}{}",
        String::from_utf8_lossy(&import_out.stdout),
        String::from_utf8_lossy(&import_out.stderr)
    );
    assert_eq!(
        import_out.status.code(),
        Some(0),
        "mgc import bun.lock must succeed:\n{import_output}"
    );
    assert!(
        sandbox.path().join("mgc.lock").exists(),
        "mgc.lock must exist after import"
    );

    let (addr, _server) = spawn_mock_npm_registry();
    let (code, output) = run_mgc_audit_mock_registry(&mgc, sandbox.path(), &addr);
    assert_eq!(
        code,
        Some(1),
        "migrated mgc.lock vulnerable pin must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("lodash"),
        "the affected package must appear:\n{output}"
    );
    assert!(
        output.contains("4.17.21"),
        "the patched boundary must appear:\n{output}"
    );
}

#[test]
fn audit_web_rival_lockfile_without_mgc_lock_fails_closed_cli() {
    // P0-3 fail-closed CLI evidence: audit trên project chỉ có bun.lock
    // (chưa import) phải TỪ CHỐI với remediation `mgc import` — không
    // âm thầm audit lockfile đối thủ.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"rival-only\"\necosystem = \"web\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("bun.lock"),
        r#"{
  "lockfileVersion": 1,
  "packages": {
    "lodash": ["lodash@4.17.20", "", {}, "sha512-x"],
  }
}"#,
    )
    .unwrap();

    let (addr, _server) = spawn_mock_npm_registry();
    let (code, output) = run_mgc_audit_mock_registry(&mgc, sandbox.path(), &addr);
    assert_ne!(
        code,
        Some(0),
        "audit must refuse rival lockfile without mgc.lock:\n{output}"
    );
    assert!(
        output.contains("mgc.lock is missing") && output.contains("mgc import"),
        "remediation must point at mgc import:\n{output}"
    );
}

#[test]
fn audit_web_deno_lock_via_cli_reports_mock_advisory_exit_1() {
    // P0-3: deno.lock → `mgc import` → mgc.lock → audit exit 1.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"deno-fixture\"\necosystem = \"web\"\n",
    )
    .unwrap();
    // Real deno.lock v5 npm section (captured 2026-09-10).
    std::fs::write(
        sandbox.path().join("deno.lock"),
        r#"{
  "version": "5",
  "specifiers": {},
  "npm": {
    "lodash@4.17.20": "4.17.20"
  }
}"#,
    )
    .unwrap();

    let import_out = Command::new(&mgc)
        .arg("import")
        .current_dir(sandbox.path())
        .output()
        .expect("spawn mgc import");
    let import_output = format!(
        "{}{}",
        String::from_utf8_lossy(&import_out.stdout),
        String::from_utf8_lossy(&import_out.stderr)
    );
    assert_eq!(
        import_out.status.code(),
        Some(0),
        "mgc import deno.lock must succeed:\n{import_output}"
    );

    let (addr, _server) = spawn_mock_npm_registry();
    let (code, output) = run_mgc_audit_mock_registry(&mgc, sandbox.path(), &addr);
    assert_eq!(
        code,
        Some(1),
        "migrated deno mgc.lock vulnerable pin must exit 1, got {code:?}:\n{output}"
    );
    assert!(
        output.contains("lodash"),
        "the affected package must appear:\n{output}"
    );
}

#[test]
fn audit_web_bun_lock_clean_pin_exit_0() {
    // P0-3: bun.lock clean pin → import → mgc.lock → audit exit 0.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"bun-clean\"\necosystem = \"web\"\n",
    )
    .unwrap();
    // 4.17.21 is OUTSIDE <4.17.21 — the advisory does not apply.
    std::fs::write(
        sandbox.path().join("bun.lock"),
        r#"{
  "lockfileVersion": 1,
  "packages": {
    "lodash": ["lodash@4.17.21", "", {}, "sha512-ok"],
  }
}"#,
    )
    .unwrap();

    let import_out = Command::new(&mgc)
        .arg("import")
        .current_dir(sandbox.path())
        .output()
        .expect("spawn mgc import");
    assert_eq!(import_out.status.code(), Some(0), "mgc import must succeed");

    let (addr, _server) = spawn_mock_npm_registry();
    let (code, output) = run_mgc_audit_mock_registry(&mgc, sandbox.path(), &addr);
    assert_eq!(code, Some(0), "non-matching pin must exit 0:\n{output}");
    assert!(
        output.contains("No vulnerabilities"),
        "clean run must say so:\n{output}"
    );
}

#[test]
fn audit_web_mock_registry_down_strict_exits_2() {
    // Tool-failure evidence for the npm-bulk-advisory lane: a DEAD
    // registry (port 1 — connection refused instantly) must surface as
    // scanner failure; strict mode turns it into exit 2, never a fake
    // clean. Deterministic — no live-network dependency.
    // Evidence tool-failure cho lane npm-bulk-advisory: registry CHẾT
    // (port 1 — refused tức thì) phải thành scanner failure; strict
    // chuyển thành exit 2, không bao giờ sạch giả. Tất định.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"bun-dead-registry\"\necosystem = \"web\"\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("bun.lock"),
        r#"{
  "lockfileVersion": 1,
  "packages": {
    "lodash": ["lodash@4.17.20", "", {}, "sha512-x"],
  }
}"#,
    )
    .unwrap();

    // P0-3: migration trước khi audit (bun.lock → mgc.lock).
    let import_out = Command::new(&mgc)
        .arg("import")
        .current_dir(sandbox.path())
        .output()
        .expect("spawn mgc import");
    assert_eq!(import_out.status.code(), Some(0), "mgc import must succeed");

    let out = Command::new(&mgc)
        .arg("audit")
        .current_dir(sandbox.path())
        .env("MAGICORE_WEB_REGISTRY_URL", "http://127.0.0.1:1")
        .env("MGC_AUDIT_STRICT", "1")
        .output()
        .expect("spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "strict + dead registry must exit 2 (environment), got {:?}:\n{text}",
        out.status.code()
    );
    assert!(
        text.contains("UNVERIFIED"),
        "the unverified state must be loud:\n{text}"
    );
}
