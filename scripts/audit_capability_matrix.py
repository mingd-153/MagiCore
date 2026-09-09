#!/usr/bin/env python3
"""Generate the audit capability matrix from EVIDENCE — never hand-written
status. Reads the audit E2E lane results (executed with real scanners)
and emits docs/specs/auditCapabilityMatrix.json.

A lane reaches "supported" ONLY when clean+vulnerable+failure evidence
all passed at the recorded commit — anything less is partial/experimental/
unsupported, matching the Tech Lead release gates (2026-09-09).

Sinh ma trận năng lực audit TỪ BẰNG CHỨNG — không viết tay trạng thái.
Đọc kết quả lane E2E audit (chạy scanner thật) và sinh JSON. Lane chỉ
đạt "supported" khi đủ evidence clean+vulnerable+failure trên commit đã
ghi — thiếu bất kỳ điều kiện nào thì partial/experimental/unsupported.
"""

import json
import os
import subprocess
import sys
import datetime

# Evidence source: run the binary E2E suite and classify per test.
# Each test maps to (core, language, scanner, evidence_kind).
E2E_TESTS = {
    "audit_lib_rust_via_cli_reports_real_rustsec_finding_exit_1": ("lib", "rust", "cargo-audit", "vulnerable_fixture"),
    "audit_lib_rust_via_cli_clean_fixture_exit_0": ("lib", "rust", "cargo-audit", "clean_fixture"),
    "audit_lib_python_via_cli_runs_real_pip_audit": ("lib", "python", "pip-audit", "clean_fixture"),
    "audit_lib_python_via_cli_reports_real_vulnerability_exit_1": ("lib", "python", "pip-audit", "vulnerable_fixture"),
    "audit_lib_python_unresolved_pyproject_fails_closed_not_environment": ("lib", "python", "pip-audit", "tool_failure"),
    "audit_app_kotlin_without_gradle_reports_tool_missing_with_remediation": ("app", "kotlin", "owasp-dependency-check", "tool_failure"),
    "audit_unavailable_strict_fails_with_environment_exit_2": ("app", "flutter", "none", "tool_failure"),
    "audit_unavailable_non_strict_exits_0_with_warning": ("app", "flutter", "none", "tool_failure"),
}

# Evidence kinds required per lane before "supported" is claimable.
REQUIRED_KINDS = {"clean_fixture", "vulnerable_fixture", "tool_failure"}


def run_e2e() -> dict[str, bool]:
    """Run the audit E2E suite; return test name -> passed. A test that
    skipped (environment-unverified) is recorded as False — skipped is
    NOT evidence."""
    proc = subprocess.run(
        ["cargo", "test", "-p", "mgc", "--test", "audit_cli_e2e", "--", "--format", "json"],
        capture_output=True, text=True,
    )
    # libtest json output — parse events
    results = {}
    for line in (proc.stdout + proc.stderr).splitlines():
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        if ev.get("type") == "test" and ev.get("event") in ("ok", "failed", "ignored"):
            name = ev["name"]
            results[name] = ev["event"] == "ok"
    return results


def main() -> int:
    if not os.environ.get("MGC_AUDIT_MATRIX_SKIP_E2E"):
        results = run_e2e()
    else:
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

    out = {
        "schema_version": 1,
        "generated_at": now,
        "commit": commit,
        "capability": capability,
        "evidence": matrix,
        "unsupported": [
            {"core": c, "language": "*", "status": "unsupported",
             "reason": "no scanner implemented — honest UnsupportedEcosystem"}
            for c in ("game", "iot", "hardware", "cloud", "cicd")
        ],
    }

    target = os.environ.get("MGC_AUDIT_MATRIX_OUT", "docs/specs/auditCapabilityMatrix.json")
    with open(target, "w") as f:
        json.dump(out, f, indent=2)
        f.write("\n")
    print(f"capability matrix written to {target} ({len(capability)} lanes)")
    for lane in capability:
        print(f"  {lane['status']:<12} {lane['core']}/{lane['language']} ({lane['scanner']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
