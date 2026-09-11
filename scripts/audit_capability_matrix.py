#!/usr/bin/env python3
"""Generate the audit capability matrix from EVIDENCE — never hand-written
status. Reads the audit E2E lane results (executed with real scanners)
and emits docs/specs/auditCapabilityMatrix.json.

Fail-closed evidence collection (Tech Lead P0-1, 2026-09-11): any suite
that exits non-zero, times out, or prints unparseable/empty output FAILS
matrix generation — a broken collection run must never emit a matrix that
looks like evidence. Lane status (partial/experimental) remains honest
data for the gates; collection INTEGRITY failures abort the script.

The expected release manifest (EXPECTED_LANES) is a code constant,
independent from test results: test outcomes never define the required
scope — the manifest defines what MUST exist, evidence only decides
whether each required lane is green.

Sinh ma trận năng lực audit TỪ BẰNG CHỨNG — không viết tay trạng thái.
Thu thập evidence fail-closed: suite exit != 0, timeout, hoặc output
không parse được → script FAIL, không sinh matrix giả. Manifest release
kỳ vọng là HẰNG SỐ trong code, độc lập với kết quả test — kết quả test
chỉ quyết định lane xanh hay không, không quyết định scope cần test.
"""

import datetime
import json
import os
import re
import subprocess
import sys

# Evidence source: run the binary E2E suite and classify per test.
# Each test maps to (core, language, scanner, evidence_kind).
# Nguồn evidence: chạy suite E2E binary rồi phân loại theo test —
# mỗi test ánh xạ sang (core, ngôn ngữ, scanner, loại evidence).
E2E_TESTS = {
    "audit_lib_rust_via_cli_reports_real_rustsec_finding_exit_1": ("lib", "rust", "cargo-audit", "vulnerable_fixture"),
    "audit_lib_rust_via_cli_clean_fixture_exit_0": ("lib", "rust", "cargo-audit", "clean_fixture"),
    "audit_lib_python_via_cli_runs_real_pip_audit": ("lib", "python", "pip-audit", "clean_fixture"),
    "audit_lib_python_via_cli_reports_real_vulnerability_exit_1": ("lib", "python", "pip-audit", "vulnerable_fixture"),
    "audit_lib_python_unresolved_pyproject_fails_closed_not_environment": ("lib", "python", "pip-audit", "tool_failure"),
    "audit_app_kotlin_without_gradle_reports_tool_missing_with_remediation": ("app", "kotlin", "owasp-dependency-check", "tool_failure"),
    "audit_unavailable_strict_fails_with_environment_exit_2": ("app", "flutter", "none", "tool_failure"),
    "audit_unavailable_non_strict_exits_0_with_warning": ("app", "flutter", "none", "tool_failure"),
    # Go lane (P1 2026-09-09): govulncheck via the shared engine.
    "audit_lib_go_via_cli_reports_real_osv_finding_exit_1": ("lib", "go", "govulncheck", "vulnerable_fixture"),
    "audit_lib_go_via_cli_clean_fixture_exit_0": ("lib", "go", "govulncheck", "clean_fixture"),
    "audit_lib_go_without_govulncheck_reports_tool_missing": ("lib", "go", "govulncheck", "tool_failure"),
    # Rust tool-missing lane (P1): completes the three-evidence set.
    "audit_lib_rust_without_cargo_audit_reports_tool_missing": ("lib", "rust", "cargo-audit", "tool_failure"),
    # P2 2026-09-10 — OSV-backed lanes (network advisories, no toolchain):
    # Java (Maven ecosystem), .NET (NuGet), Swift (SwiftURL — with the
    # naming fix), Dart/Pub (flutter).
    "audit_lib_java_via_cli_reports_real_osv_finding_exit_1": ("lib", "java", "osv-maven", "vulnerable_fixture"),
    "audit_lib_java_via_cli_clean_fixture_exit_0": ("lib", "java", "osv-maven", "clean_fixture"),
    "audit_lib_dotnet_via_cli_reports_real_osv_finding_exit_1": ("lib", "dotnet", "osv-nuget", "vulnerable_fixture"),
    "audit_lib_dotnet_via_cli_clean_fixture_exit_0": ("lib", "dotnet", "osv-nuget", "clean_fixture"),
    "audit_app_swift_via_cli_reports_real_osv_findings_exit_1": ("app", "swift", "osv-swifturl", "vulnerable_fixture"),
    "audit_app_flutter_via_cli_reports_real_osv_finding_exit_1": ("app", "flutter", "osv-pub", "vulnerable_fixture"),
    # Clean fixtures for the OSV-backed app lanes (P2 2026-09-10).
    "audit_app_swift_via_cli_clean_fixture_exit_0": ("app", "swift", "osv-swifturl", "clean_fixture"),
    "audit_app_flutter_via_cli_clean_fixture_exit_0": ("app", "flutter", "osv-pub", "clean_fixture"),
    # Offline/dead-endpoint tool-failure evidence for the OSV-backed
    # lanes (MGC_OSV_API_BASE override — deterministic, no flaky network).
    # Evidence tool-failure cho lane OSV: endpoint chết tất định qua
    # MGC_OSV_API_BASE, không phụ thuộc mạng chập chờn.
    "audit_osv_unreachable_strict_fails_with_environment_exit_2": ("app", "flutter", "osv-pub", "tool_failure"),
    "audit_osv_unreachable_non_strict_exits_0_with_warning": ("lib", "dotnet", "osv-nuget", "tool_failure"),
    "audit_osv_unreachable_java_strict_exit_2": ("lib", "java", "osv-maven", "tool_failure"),
    "audit_osv_unreachable_swift_strict_exit_2": ("app", "swift", "osv-swifturl", "tool_failure"),
    # Bun/Deno LOCKFILE ADVISORY PARSER lanes (P0-3 rename 2026-09-10):
    # these are lockfile-import → mgc.lock → advisory E2Es (migration
    # compatibility evidence), NOT native-engine binary E2Es. Bun/Deno
    # binaries are migration/fixture tools only — they never run the
    # default mgc dev/test/run/build lanes.
    # Lane lockfile advisory parser Bun/Deno: E2E import lockfile →
    # mgc.lock → advisory (evidence migration compatibility), KHÔNG phải
    # E2E binary native-engine. Bun/Deno chỉ là tool fixture/migration.
    "audit_web_bun_lock_via_cli_reports_mock_advisory_exit_1": ("web", "javascript", "npm-bulk-advisory", "vulnerable_fixture"),
    "audit_web_bun_lock_clean_pin_exit_0": ("web", "javascript", "npm-bulk-advisory", "clean_fixture"),
    "audit_web_deno_lock_via_cli_reports_mock_advisory_exit_1": ("web", "javascript", "npm-bulk-advisory", "vulnerable_fixture"),
    "audit_web_mock_registry_down_strict_exits_2": ("web", "javascript", "npm-bulk-advisory", "tool_failure"),
    # Process-spawn audit lane (native engine, Tech Lead #7, P0-1
    # rewrite): hermetic PATH-canary refusals are the execution-engine
    # evidence — canary silence proves NO rival process ever spawned.
    "mgc_test_never_spawns_bun_deno_or_pm_from_package_script": ("*", "*", "native-engine", "tool_failure"),
    "mgc_dev_never_spawns_bun_or_deno_by_default_process_canary": ("*", "*", "native-engine", "tool_failure"),
    "mgc_run_never_spawns_rival_runtime_without_compat_flag": ("*", "*", "native-engine", "tool_failure"),
    "mgc_build_never_spawns_rival_runtime_or_pm_from_build_script": ("*", "*", "native-engine", "tool_failure"),
}

# Evidence kinds required per lane before "supported" is claimable.
# Các loại evidence bắt buộc cho mỗi lane trước khi được "supported".
REQUIRED_KINDS = {"clean_fixture", "vulnerable_fixture", "tool_failure"}

# Expected release capability manifest — INDEPENDENT from test results
# (Tech Lead P0-1/P0-2, 2026-09-11). This constant is the reviewable
# scope contract: every lane here MUST exist in the generated matrix and
# reach "supported" before a release gate may pass. Test results decide
# whether each lane is green — they NEVER define the required scope.
#
# Deliberately NOT in the manifest (never "supported" by design, so they
# must never be release-required):
#   - app/kotlin owasp-dependency-check: tool_failure-only lane (OWASP
#     CLI not provisioned in release jobs) — honest partial.
#   - app/flutter "none": audit-unavailable exit-contract lane, carries
#     only tool_failure evidence by definition.
#   - ("*", "*", "native-engine"): execution-policy spawn canary — its
#     suite must EXIT 0 (checked in run_e2e), but it has no clean/vuln
#     fixture kinds, so "supported" is not a meaningful status for it.
#
# Bản manifest năng lực release KỲ VỌNG — ĐỘC LẬP với kết quả test: mọi
# lane ở đây PHẢI tồn tại trong matrix và đạt "supported" thì release
# gate mới được qua. Kết quả test chỉ quyết định lane xanh — không bao
# giờ quyết định scope phải test.
EXPECTED_LANES = [
    ("lib", "rust", "cargo-audit"),
    ("lib", "python", "pip-audit"),
    ("lib", "go", "govulncheck"),
    ("lib", "java", "osv-maven"),
    ("lib", "dotnet", "osv-nuget"),
    ("app", "swift", "osv-swifturl"),
    ("app", "flutter", "osv-pub"),
    ("web", "javascript", "npm-bulk-advisory"),
]

# Per-suite timeout (seconds). Overridable via MGC_AUDIT_MATRIX_SUITE_TIMEOUT
# (RULE §12 — defaults are named constants, never inline literals).
# Timeout mỗi suite (giây) — ghi đè qua MGC_AUDIT_MATRIX_SUITE_TIMEOUT.
SUITE_TIMEOUT_DEFAULT_S = 3600

# libtest summary line: "test result: ok. 29 passed; 0 failed; ..." —
# used to prove the suite actually executed a non-empty test set.
# Dòng tóm tắt libtest — chứng minh suite đã chạy test thật (không rỗng).
_SUMMARY_RE = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed")


def _fail_collection(message: str, detail: str = "") -> None:
    """Abort matrix generation — a broken evidence run must never emit
    a matrix that looks like evidence (fail-closed collection).
    Huỷ sinh matrix — evidence hỏng thì không bao giờ sinh matrix giả."""
    print(f"EVIDENCE COLLECTION FAILED: {message}", file=sys.stderr)
    if detail:
        print(detail[-4000:], file=sys.stderr)
    sys.exit(1)


def run_e2e() -> dict[str, bool]:
    """Run the audit E2E suites; return test name -> passed. A test that
    skipped (environment-unverified) is recorded as False — skipped is
    NOT evidence. Parses the STABLE `test <name> ... ok/FAILED` text
    output, PLUS the explicit skip markers the guards print as
    `SKIP (environment-unverified) test=<name>` (a guard skip still ends
    libtest "ok", so the marker is the only honest downgrade signal).
    Any suite exiting non-zero, timing out, or printing unparseable/
    empty output fails generation outright (P0-1 fail-closed).
    Chạy các suite E2E audit; trả tên test -> pass. Test skip thì ghi
    False — skip KHÔNG PHẢI evidence. Suite exit != 0, timeout, hoặc
    output không parse được / rỗng → huỷ sinh matrix ngay (fail-closed)."""
    timeout_s = int(os.environ.get("MGC_AUDIT_MATRIX_SUITE_TIMEOUT", SUITE_TIMEOUT_DEFAULT_S))
    results: dict[str, bool] = {}
    # Both suites contribute evidence: the scanner lanes (audit_cli_e2e)
    # AND the native-engine spawn audit (Tech Lead #7).
    # Cả hai suite đóng góp evidence: lane scanner VÀ audit spawn.
    suites = [
        ["cargo", "test", "-p", "mgc", "--test", "audit_cli_e2e", "--", "--test-threads=1", "--nocapture"],
        ["cargo", "test", "-p", "mgc", "--test", "native_engine_spawn_audit", "--", "--nocapture"],
    ]
    combined = ""
    for suite in suites:
        label = " ".join(suite)
        try:
            proc = subprocess.run(suite, capture_output=True, text=True, timeout=timeout_s)
        except subprocess.TimeoutExpired:
            _fail_collection(f"suite timed out after {timeout_s}s: {label}")
        if proc.returncode != 0:
            _fail_collection(
                f"suite exited with code {proc.returncode}: {label}",
                (proc.stdout or "") + "\n" + (proc.stderr or ""),
            )
        output = (proc.stdout or "") + "\n" + (proc.stderr or "") + "\n"

        # Parseability proof: a real libtest summary with a non-zero
        # executed count must exist (exit 0 with no recognizable
        # summary = unparseable — fail-closed, never guess).
        # Bằng chứng parse được: phải có dòng tóm tắt libtest thật với
        # số test đã chạy > 0 (exit 0 mà không có summary = fail-closed).
        summaries = _SUMMARY_RE.findall(output)
        if not summaries:
            _fail_collection(f"no libtest summary parsed (unparseable output): {label}", output)
        executed = sum(int(p) + int(f) for _, p, f in summaries)
        if executed == 0:
            _fail_collection(f"suite executed zero tests: {label}", output)
        for verdict, _, _ in summaries:
            if verdict == "FAILED":
                _fail_collection(f"libtest summary says FAILED despite exit 0: {label}", output)

        combined += output
    for line in combined.splitlines():
        line = line.strip()
        # Stable libtest text line: "test <name> ... ok" | "... FAILED" |
        # "... ignored" (skip). Skipped/ignored is NOT passed evidence.
        # Dòng text libtest ổn định: ok/FAILED/ignored (skip không phải
        # evidence pass).
        if not line.startswith("test ") or " ... " not in line:
            continue
        rest = line[len("test "):]
        if " ... " not in rest:
            continue
        name, verdict = rest.rsplit(" ... ", 1)
        if verdict.startswith("ok"):
            results[name] = True
        elif verdict.startswith("FAILED") or verdict.startswith("ignored"):
            results[name] = False

    # SILENT-SKIP FIX (Tech Lead: skip ≠ evidence): any test that
    # printed the explicit skip marker is downgraded to unverified even
    # though libtest reported "ok".
    # FIX SKIP ÂM THẦN: test in marker skip tường minh thì bị hạ cấp
    # unverified dù libtest báo "ok".
    for m in re.finditer(
        r"SKIP \(environment-unverified\) test=([A-Za-z0-9_]+)", combined
    ):
        results[m.group(1)] = False
    return results


def main() -> int:
    if not os.environ.get("MGC_AUDIT_MATRIX_SKIP_E2E"):
        results = run_e2e()
    else:
        # Escape hatch (dev-only): regenerate the matrix shell without
        # running suites. The output can NEVER pass a release gate —
        # every expected lane will be missing/unverified evidence.
        # Cửa thoát (chỉ để dev): sinh vỏ matrix không chạy suite —
        # output không bao giờ qua được release gate.
        results = {}

    commit = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    matrix: list[dict] = []
    for name, (core, lang, scanner, kind) in E2E_TESTS.items():
        passed = results.get(name, False)
        matrix.append({
            "core": core,
            "language": lang,
            "scanner": scanner,
            "evidence_kind": kind,
            "test": name,
            "status": "passed" if passed else "environment-unverified",
            "commit": commit,
            "verified_at": now,
        })

    # Aggregate per lane: supported only with every required kind passing.
    lanes: dict[tuple, dict] = {}
    for e in matrix:
        key = (e["core"], e["language"], e["scanner"])
        lanes.setdefault(key, {"kinds": set(), "all_passed": True, "entries": []})
        if e["evidence_kind"] in REQUIRED_KINDS:
            lanes[key]["kinds"].add(e["evidence_kind"])
            if e["status"] != "passed":
                lanes[key]["all_passed"] = False
        lanes[key]["entries"].append(e)

    capability = []
    for (core, lang, scanner), lane in sorted(lanes.items()):
        if lane["kinds"] >= REQUIRED_KINDS and lane["all_passed"]:
            status = "supported"
        elif lane["kinds"] and lane["all_passed"]:
            status = "partial"
        else:
            status = "experimental"
        capability.append({
            "core": core, "language": lang, "scanner": scanner, "status": status,
            "evidence_kinds": sorted(lane["kinds"]),
            "commit": commit, "verified_at": now,
        })

    # Expected-manifest verification (P0-1): every required lane must
    # EXIST in the matrix. Missing lanes are recorded explicitly so the
    # release gate can reject them — results never shrink the scope.
    # Kiểm tra manifest kỳ vọng: mọi lane bắt buộc phải TỒN TẠI trong
    # matrix; lane thiếu bị ghi đích danh để release gate chặn.
    missing = [
        {"core": c, "language": l, "scanner": s,
         "reason": "required lane has no evidence at this commit"}
        for (c, l, s) in EXPECTED_LANES if (c, l, s) not in lanes
    ]

    out = {
        "schema_version": 1,
        "generated_at": now,
        "commit": commit,
        # The reviewable scope contract, echoed for the gates to enforce.
        # Hợp đồng scope để gate thực thi — độc lập với kết quả test.
        "expected_lanes": [
            {"core": c, "language": l, "scanner": s} for (c, l, s) in EXPECTED_LANES
        ],
        "missing_lanes": missing,
        "capability": capability,
        "evidence": matrix,
        "unsupported": [
            # Honest per-ecosystem gaps (2026-09-10): these cores RUN
            # (polyglot engine scans sibling manifests) but their
            # TARGET-ecosystem lanes are partial or absent — named
            # explicitly, never a blanket "no scanner".
            # Các core này CHẠY (engine polyglot quét manifest kề) nhưng
            # lane ecosystem ĐÍCH còn thiếu — liệt kê đích danh.
            {"core": "game", "language": "unity/unreal", "status": "unsupported",
             "reason": "no Unity/Unreal advisory scanner exists (UPM/native plugin audit unsolved upstream) — Bevy (Cargo.toml) rides the polyglot cargo-audit lane"},
            {"core": "iot", "language": "c/c++/micropython", "status": "unsupported",
             "reason": "no C/C++ manifest SBOM standard adopted (conan/vcpkg not wired) — Rust IoT projects ride the polyglot cargo-audit lane"},
            {"core": "hardware", "language": "fpga/hdl", "status": "unsupported",
             "reason": "toolchain checksum + HDL/IP provenance lane not implemented"},
            {"core": "cloud", "language": "kubernetes/docker", "status": "unsupported",
             "reason": "container/IaC image SBOM scanning not implemented — Terraform provider-lock provenance lane EXISTS (checksum-enforced)"},
            {"core": "cicd", "language": "gitlab/jenkins", "status": "unsupported",
             "reason": "plugin/image/script provenance not implemented — GitHub Actions policy lane EXISTS (SHA pinning/permissions/injection)"},
            {"core": "app", "language": "objc/cocoapods", "status": "unsupported",
             "reason": "OSV.dev carries no CocoaPods advisory ecosystem (verified live: 'pods' rejected) — no advisory database exists to query yet"},
        ],
    }

    target = os.environ.get("MGC_AUDIT_MATRIX_OUT", "docs/specs/auditCapabilityMatrix.json")
    with open(target, "w") as f:
        json.dump(out, f, indent=2)
        f.write("\n")
    print(f"capability matrix written to {target} ({len(capability)} lanes)")
    for lane in capability:
        print(f"  {lane['status']:<12} {lane['core']}/{lane['language']} ({lane['scanner']})")
    for m in missing:
        print(f"  MISSING      {m['core']}/{m['language']} ({m['scanner']}) — required lane absent")
    print(f"expected release lanes: {len(EXPECTED_LANES)}, missing: {len(missing)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
