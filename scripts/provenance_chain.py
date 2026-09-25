#!/usr/bin/env python3
"""Provenance chain tracker (P0-4 — Tech Lead batch 2026-09-16 11:52).

A release must be ONE SHA all the way through: source commit → tag →
CI runs → lifecycle-matrix evidence → artifacts → SBOM → attestation.
This script materializes that chain as the gitignored JSON
docs/specs/provenanceChain.json and FAILS CLOSED while any link is
still pending: the default run exits 1 with the exact pending list, and
`--verify` re-checks the stored chain without regenerating it. Nothing
may be tagged or published while this gate is red.

Trạng thái chuỗi provenance P0-4: một release phải là MỘT SHA xuyên
suốt — commit nguồn → tag → lần chạy CI → evidence lifecycle matrix →
artifact → SBOM → attestation. Script này hiện thực hóa chuỗi đó thành
JSON gitignored docs/specs/provenanceChain.json và FAIL-CLOSED khi còn
link pending: lần chạy mặc định exit 1 kèm danh sách pending chính xác,
`--verify` kiểm lại chain đã lưu mà không regenerate. Không được tag hay
publish gì khi cổng này còn đỏ.
"""

import datetime
import json
import re
import subprocess
import sys
from pathlib import Path

# Repository root (this script lives in scripts/ — same convention as
# verify_release_manifests.py).
# (Gốc repo — script nằm trong scripts/, cùng quy ước
# verify_release_manifests.py.)
ROOT = Path(__file__).resolve().parent.parent
OUT_PATH = ROOT / "docs" / "specs" / "provenanceChain.json"
MATRIX_PATH = ROOT / "docs" / "specs" / "lifecycleCapabilityMatrix.json"

# Workspace version: the first `version = "..."` in the root Cargo.toml
# (mirrors Gate E's parser — the same single source of truth).
# (Version workspace: khóa `version = "..."` đầu tiên trong Cargo.toml gốc
# — khuôn parser của Gate E, cùng một nguồn chân lý.)
WORKSPACE_VERSION_RE = re.compile(r'^version\s*=\s*"([^"]+)"', re.M)

# The chain links every publish must prove. Extra bookkeeping fields
# (pending, pending_fields, notes, generated_at) are written alongside.
# (Các mắt xích mà mọi lần publish phải chứng minh. Trường bookkeeping
# (pending, pending_fields, notes, generated_at) ghi kèm.)
REQUIRED_FIELDS = (
    "commit_sha", "describe", "cargo_version", "tag",
    "ci_runs", "matrix_sha", "artifact_shas", "sbom", "attestation",
)

NOTES = (
    "P0-4 provenance chain — one SHA from source to published artifact. "
    "Auto-filled here: commit_sha/describe/cargo_version/tag/matrix_sha. "
    "Filled later by CI/humans: ci_runs (verifying run ids/urls), "
    "artifact_shas (published sha256 per asset), sbom (CycloneDX JSON), "
    "attestation (GitHub provenance entry). pending=true means the chain "
    "is INCOMPLETE — this gate and verify_release_manifests.py (Gate E) "
    "must both be green before any tag/publish. "
    "(Chuỗi provenance P0-4 — một SHA từ source tới artifact công khai. "
    "Tự điền: commit_sha/describe/cargo_version/tag/matrix_sha. Điền sau "
    "bởi CI/người: ci_runs (id/url run verify), artifact_shas (sha256 đã "
    "publish theo asset), sbom (CycloneDX JSON), attestation (provenance "
    "của GitHub). pending=true nghĩa là chuỗi CHƯA ĐỦ — cổng này và "
    "verify_release_manifests.py (Gate E) phải cùng xanh trước khi tag/"
    "publish bất cứ gì.)"
)


def _git(*args: str) -> str:
    proc = subprocess.run(["git", *args], capture_output=True, text=True, cwd=ROOT)
    return (proc.stdout or "").strip() if proc.returncode == 0 else ""


def _workspace_version() -> str:
    try:
        text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    except OSError:
        return ""
    match = WORKSPACE_VERSION_RE.search(text)
    return match.group(1) if match else ""


def _matrix_sha_for_head(head: str):
    """(sha_or_none, is_current): the lifecycle matrix's recorded commit,
    plus whether it matches HEAD — a matrix collected on another SHA
    proves nothing about THIS commit (the same-SHA contract).
    ((sha, is_current): commit đã ghi trong lifecycle matrix, kèm cờ khớp
    HEAD — matrix collect trên SHA khác không chứng minh gì cho commit
    NÀY (hợp đồng same-SHA).)"""
    try:
        data = json.loads(MATRIX_PATH.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None, False
    if not isinstance(data, dict):
        return None, False
    commit = data.get("commit")
    if not isinstance(commit, str) or not commit:
        return None, False
    return commit, commit == head


def collect() -> dict:
    """Regenerate the chain from the CURRENT machine state; every link
    that cannot be proven right now stays pending.
    (Regenerate chain từ trạng thái MÁY HIỆN TẠI; mắt xích nào chưa chứng
    minh được ngay thì giữ pending.)"""
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    head = _git("rev-parse", "HEAD")
    describe = _git("describe", "--tags", "--always", "--dirty")
    version = _workspace_version()
    tags = sorted(t for t in _git("tag", "--points-at", "HEAD").splitlines() if t)
    matrix_sha, matrix_current = _matrix_sha_for_head(head)

    chain = {
        "commit_sha": head or None,
        "describe": describe or None,
        "cargo_version": version or None,
        # The tag pointing at HEAD — RC6 is deliberately NOT cut yet
        # (creation waits for a fully green verify ladder + approval), so
        # this stays null until the verified tag exists.
        # (Tag trỏ vào HEAD — RC6 cố ý CHƯA tạo (việc tạo chờ verify
        # ladder xanh trọn vẹn + phê duyệt), nên trường này là null tới
        # khi tag đã verify tồn tại.)
        "tag": tags[0] if tags else None,
        # CI runs that verified THIS commit — appended once CI is green
        # on the same SHA. An empty list is pending, never a pass.
        # (Các lần chạy CI đã verify CHÍNH commit này — bổ sung khi CI
        # xanh trên cùng SHA. List rỗng là pending, không bao giờ là pass.)
        "ci_runs": [],
        "matrix_sha": matrix_sha,
        "artifact_shas": {},
        "sbom": None,
        "attestation": None,
        "notes": NOTES,
        "generated_at": now,
    }

    pending = []
    if not chain["commit_sha"]:
        pending.append("commit_sha (git unavailable or not a repo)")
    if not chain["describe"]:
        pending.append("describe (git describe failed)")
    if not chain["cargo_version"]:
        pending.append("cargo_version (root Cargo.toml version key missing)")
    if not chain["tag"]:
        pending.append("tag (no tag points at HEAD — RC6 not cut yet)")
    if not chain["ci_runs"]:
        pending.append("ci_runs (no CI run recorded for this commit)")
    if not matrix_sha:
        pending.append("matrix_sha (lifecycle matrix missing or has no commit)")
    elif not matrix_current:
        pending.append(
            f"matrix_sha (matrix ran on {matrix_sha[:12]}, not HEAD {head[:12] if head else '?'})"
        )
    if not chain["artifact_shas"]:
        pending.append("artifact_shas (no release artifacts published/hashed)")
    if chain["sbom"] is None:
        pending.append("sbom (no SBOM recorded)")
    if chain["attestation"] is None:
        pending.append("attestation (no provenance attestation recorded)")

    chain["pending"] = bool(pending)
    chain["pending_fields"] = pending
    return chain


def write(chain: dict) -> None:
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_PATH.write_text(json.dumps(chain, indent=2) + "\n", encoding="utf-8")


def _report(chain: dict) -> None:
    head = str(chain.get("commit_sha") or "unknown")
    print(f"provenance chain: commit={head[:12]} "
          f"version={chain.get('cargo_version')} tag={chain.get('tag')}")
    # A non-null value can still be PENDING (e.g. a matrix_sha that
    # records another commit) — the pending list is the truth the table
    # must mirror, never the mere presence of a value.
    # (Giá trị không-null vẫn có thể PENDING (vd matrix_sha ghi commit
    # khác) — danh sách pending mới là sự thật mà bảng phải phản chiếu,
    # không phải sự hiện diện của giá trị.)
    pending_keys = {p.split(" ")[0] for p in chain.get("pending_fields", [])}
    for field in REQUIRED_FIELDS:
        value = chain.get(field)
        if field in pending_keys or value in (None, "", [], {}):
            state, shown = "PENDING", "-"
        else:
            state, shown = "OK", value
        print(f"  {field:<14} {state:<8} {shown}")
    if chain.get("pending"):
        print(
            f"provenance chain INCOMPLETE: {len(chain['pending_fields'])} pending "
            "field(s) — TAG/PUBLISH BLOCKED (fail-closed, P0-4)"
        )
    else:
        print("provenance chain COMPLETE — one SHA from source to attestation")


def main() -> int:
    if "--verify" in sys.argv[1:]:
        # Gate mode: check the EXISTING chain (no regeneration) — fail
        # closed when the file is missing, unparseable, or a required
        # field vanished (schema drift).
        # (Chế độ cổng: kiểm chain ĐÃ CÓ (không regenerate) — fail-closed
        # khi thiếu file, JSON hỏng, hoặc mất field bắt buộc (schema
        # drift).)
        if not OUT_PATH.is_file():
            print(
                f"PROVENANCE GATE: {OUT_PATH} not found — run "
                "`python3 scripts/provenance_chain.py` first (fail-closed)",
                file=sys.stderr,
            )
            return 1
        try:
            chain = json.loads(OUT_PATH.read_text(encoding="utf-8"))
        except ValueError as exc:
            print(f"PROVENANCE GATE: unparseable chain JSON: {exc}", file=sys.stderr)
            return 1
        if not isinstance(chain, dict):
            print("PROVENANCE GATE: chain JSON is not an object (fail-closed)",
                  file=sys.stderr)
            return 1
        missing = [f for f in REQUIRED_FIELDS if f not in chain]
        if missing:
            print(
                f"PROVENANCE GATE: required field(s) missing: {', '.join(missing)}",
                file=sys.stderr,
            )
            return 1
        _report(chain)
        return 1 if chain.get("pending") or chain.get("pending_fields") else 0

    chain = collect()
    write(chain)
    _report(chain)
    # Fail-closed by design: while any link is pending, the default run
    # exits 1 so no pipeline can mistake an incomplete chain for a pass.
    # (Fail-closed theo thiết kế: còn link pending thì lần chạy mặc định
    # exit 1 để pipeline không thể nhầm chain chưa đủ là pass.)
    return 1 if chain["pending"] else 0


if __name__ == "__main__":
    sys.exit(main())
