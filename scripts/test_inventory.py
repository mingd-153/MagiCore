#!/usr/bin/env python3
"""Test INVENTORY — machine-readable record of what the workspace test
surface actually contains and what it actually did on THIS run.

P0-6 / Tech Lead verdict 2026-09-15: "9 lanes" was never a test inventory.
This script runs `cargo test --workspace --all-features --locked
--no-fail-fast`, parses the libtest output per test binary, and emits a
JSON artifact: {commit, command, platform, exit_code, passed, failed,
ignored, suites[], skipped_reasons[]}.

Honesty contract:
- IGNORED tests are NEVER added to `passed`. `unverified` stays unverified.
- Every ignored test is classified (live-network / runtime-unavailable /
  evidence-only / other) from the `#[ignore = "..."]` reasons found in the
  source, so the JSON states WHY something did not run.
- Suites in the CI REQUIRED fault-matrix set are tagged
  `release-blocking`; everything else is informational.

(Bảng kê TEST máy-đọc: script chạy `cargo test --workspace --all-features
--locked --no-fail-fast`, parse output libtest theo test binary và sinh
JSON. Hợp đồng trung thực: test IGNORED KHÔNG BAO GIỜ được cộng vào
`passed`; mọi test ignored được phân loại (live-network /
runtime-unavailable / evidence-only / other) từ lý do `#[ignore = "..."]`
trong source; suite thuộc fault-matrix BẮT BUỘC của CI được gắn nhãn
`release-blocking`.)
"""

import argparse
import datetime
import json
import os
import platform
import re
import subprocess
import sys

DEFAULT_COMMAND = [
    "cargo", "test", "--workspace", "--all-features", "--locked",
    "--no-fail-fast",
]

# Suites the CI fault-matrix gates on (MUST PASS, no || true) — the release
# evidence set. Anything else in this inventory is informational.
# (Suite mà fault-matrix của CI gate — tập bằng chứng release. Còn lại chỉ
# mang tính tham khảo.)
RELEASE_BLOCKING_SUITES = {
    "kill_injection_matrix",
    "stress_shared_store_matrix",
    "install_process_race",
    "stress_64_process_race",
    "disk_fault_injection_e2e",
}

# Directories scanned for #[ignore = "..."] reasons (source of truth for
# WHY a test is skipped). Integration tests + inline test modules.
# (Thư mục quét lý do #[ignore = "..."] — nguồn chân lý cho VÌ SAO test
# bị skip.)
IGNORE_SCAN_ROOTS = ["cli/tests", "cli/src", "core/crates", "adapters"]

# "Running <binary-path>" lines delimit per-suite sections in libtest output.
RUNNING_RE = re.compile(r"^\s*Running\s+(?:unittests\s+)?(\S+)")
# "test result: ok. 12 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out"
RESULT_RE = re.compile(
    r"^test result: (\w+)\.\s*(\d+) passed;\s*(\d+) failed;\s*(\d+) ignored;"
    r"\s*(\d+) measured;\s*(\d+) filtered out",
)
# Per-test verdict lines inside a section.
TEST_LINE_RE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)")


def suite_name_from_binary(binary: str) -> str:
    """`target/debug/deps/kill_injection_matrix-1a2b` →
    `kill_injection_matrix`; `unittests src/lib.rs` for inline tests keeps
    the crate-relative path so the suite is still identifiable.
    (Đổi đường binary thành tên suite; test inline giữ path để nhận diện.)"""
    base = os.path.basename(binary)
    if base.endswith(".rs"):
        # Inline unittests (src/lib.rs form): suite = the path itself.
        # (Unittest inline: suite = chính path đó.)
        return binary
    # Cargo test binaries carry a `-<hash>` suffix — strip it.
    # (Binary test của cargo mang hậu tố `-<hash>` — cắt đi.)
    return re.sub(r"-[0-9a-f]{8,}$", "", base)


def scan_ignore_reasons() -> dict:
    """Map ignored-test classification → list of reasons found in source.
    (Map phân loại test ignored → danh sách lý do tìm thấy trong source.)"""
    buckets = {"live-network": [], "runtime-unavailable": [],
               "evidence-only": [], "other": []}
    for root in IGNORE_SCAN_ROOTS:
        if not os.path.isdir(root):
            continue
        for dirpath, _dirs, files in os.walk(root):
            if "/target/" in dirpath or dirpath.endswith("/target"):
                continue
            for fname in files:
                if not fname.endswith(".rs"):
                    continue
                path = os.path.join(dirpath, fname)
                try:
                    text = open(path, encoding="utf-8", errors="replace").read()
                except OSError:
                    continue
                for m in re.finditer(
                    r'#\[ignore\s*=\s*"([^"]*)"\]', text
                ):
                    reason = m.group(1).lower()
                    if "network" in reason or "crates.io" in reason or "live" in reason:
                        bucket = "live-network"
                    elif ("runtime" in reason or "requires" in reason or "missing" in reason
                          or "not installed" in reason or "toolchain" in reason
                          or "needs" in reason):
                        bucket = "runtime-unavailable"
                    elif "evidence" in reason:
                        bucket = "evidence-only"
                    else:
                        bucket = "other"
                    buckets[bucket].append({"file": path, "reason": m.group(1)})
    return buckets


def run_inventory(cargo_args: list) -> dict:
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"], capture_output=True, text=True
    ).stdout.strip()
    command = " ".join(cargo_args)
    proc = subprocess.run(cargo_args, capture_output=True, text=True)
    output = (proc.stdout or "") + "\n" + (proc.stderr or "")

    current_suite = None
    suites: dict = {}
    ignored_tests: list = []
    for line in output.splitlines():
        m = RUNNING_RE.match(line)
        if m:
            current_suite = suite_name_from_binary(m.group(1))
            suites.setdefault(current_suite, {
                "suite": current_suite,
                "gate_role": ("release-blocking"
                              if suite_name_from_binary(m.group(1)) in RELEASE_BLOCKING_SUITES
                              else "informational"),
                "passed": 0, "failed": 0, "ignored": 0,
                "failed_tests": [], "ignored_tests": [],
            })
            continue
        if current_suite:
            tm = TEST_LINE_RE.match(line.strip())
            if tm:
                name, verdict = tm.group(1), tm.group(2)
                if verdict == "ok":
                    suites[current_suite]["passed"] += 1
                elif verdict == "FAILED":
                    suites[current_suite]["failed"] += 1
                    suites[current_suite]["failed_tests"].append(name)
                else:
                    suites[current_suite]["ignored"] += 1
                    suites[current_suite]["ignored_tests"].append(name)
                    ignored_tests.append(f"{current_suite}::{name}")
                continue
            rm = RESULT_RE.match(line.strip())
            if rm:
                # Trust the libtest summary for the counts — per-test lines
                # can be truncated by interleaved output on failures.
                # (Tin tổng kết libtest cho số liệu — dòng per-test có thể
                # bị cắt khi output xen kẽ lúc fail.)
                verdict, passed, failed, ignored = (
                    rm.group(1), int(rm.group(2)), int(rm.group(3)),
                    int(rm.group(4)),
                )
                s = suites.get(current_suite)
                if s:
                    s["passed"], s["failed"], s["ignored"] = passed, failed, ignored
                else:
                    suites[current_suite] = {
                        "suite": current_suite,
                        "gate_role": ("release-blocking" if current_suite
                                      in RELEASE_BLOCKING_SUITES else "informational"),
                        "passed": passed, "failed": failed, "ignored": ignored,
                        "failed_tests": [], "ignored_tests": [],
                    }

    total_passed = sum(s["passed"] for s in suites.values())
    total_failed = sum(s["failed"] for s in suites.values())
    total_ignored = sum(s["ignored"] for s in suites.values())
    # Compile failures produce no "Running" section — surface that honestly
    # via the exit code rather than pretending the surface is green.
    # (Lỗi compile không sinh section "Running" — lộ ra trung thực qua exit
    # code thay vì giả bề mặt xanh.)
    return {
        "schema_version": 1,
        "commit": commit,
        "command": command,
        "platform": f"{platform.system()}-{platform.machine()}",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ),
        "exit_code": proc.returncode,
        "passed": total_passed,
        "failed": total_failed,
        # Ignored is counted SEPARATELY — it is never folded into passed.
        # (Ignored đếm RIÊNG — không bao giờ gộp vào passed.)
        "ignored": total_ignored,
        "suites": sorted(suites.values(), key=lambda s: s["suite"]),
        "skipped_reasons": scan_ignore_reasons(),
        "ignored_test_ids": ignored_tests,
        "_raw_tail": output[-4000:],
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default=None,
                    help="output JSON path (default: MGC_TEST_INVENTORY_OUT "
                         "or stdout)")
    ap.add_argument("--list-only", action="store_true",
                    help="scan #[ignore] reasons and exit without running "
                         "cargo (for disk-constrained environments)")
    args = ap.parse_args()

    if args.list_only:
        data = {
            "schema_version": 1,
            "commit": subprocess.run(["git", "rev-parse", "HEAD"],
                                     capture_output=True, text=True
                                     ).stdout.strip(),
            "platform": f"{platform.system()}-{platform.machine()}",
            "skipped_reasons": scan_ignore_reasons(),
            "note": "list-only: cargo not run",
        }
    else:
        data = run_inventory(DEFAULT_COMMAND)

    out = args.out or os.environ.get("MGC_TEST_INVENTORY_OUT")
    text = json.dumps(data, indent=2) + "\n"
    if out:
        os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
        with open(out, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"test inventory written to {out}")
    else:
        sys.stdout.write(text)

    if not args.list_only and data["exit_code"] != 0:
        print(f"WARNING: cargo test exited {data['exit_code']} "
              f"({data['failed']} failed tests)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
