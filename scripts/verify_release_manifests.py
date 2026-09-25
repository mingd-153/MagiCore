#!/usr/bin/env python3
"""Release manifest gate (Gate E, P0-3 — Tech Lead verdict 2026-09-16).

Every distribution manifest must satisfy TWO conditions before a publish
is allowed:
  1. its version equals the WORKSPACE version from the root Cargo.toml;
  2. it contains NO pending/unavailable checksum placeholder — every
     checksum must be a real 64-hex-char SHA256.
Any violation exits 1 (fail-closed): a manifest whose hashes point at the
wrong release assets (or at nothing) must never be publishable by
accident. Run this right before `scripts/publish.sh` / the release job.

Cổng manifest release (Gate E, P0-3): mọi manifest phân phối phải thỏa
HAI điều kiện trước khi được phép publish: (1) version khớp version
WORKSPACE từ Cargo.toml gốc; (2) KHÔNG còn placeholder checksum
PENDING/UNAVAILABLE — mọi checksum phải là SHA256 hex-64 thật. Vi phạm →
exit 1 (fail-closed): manifest trỏ hash sai asset (hoặc trỏ vào hư không)
không bao giờ được phép publish nhầm. Chạy ngay trước publish.
"""

import json
import re
import sys
from pathlib import Path

# Repository root (this script lives in scripts/).
# (Gốc repo — script này nằm trong scripts/.)
ROOT = Path(__file__).resolve().parent.parent

# The distribution manifests this gate covers — the release surface that
# would silently break installs if published with stale hashes.
# (Các manifest phân phối cổng này phủ — mặt release sẽ làm hỏng install
# lặng lẽ nếu publish kèm hash cũ.)
HOMEBREW_MANIFESTS = [
    ROOT / "packaging" / "homebrew" / "magicore.rb",
    ROOT / "packaging" / "homebrew" / "magicore-web.rb",
]
SCOOP_MANIFESTS = [
    ROOT / "packaging" / "scoop" / "magicore.json",
    ROOT / "packaging" / "scoop" / "magicore-web.json",
]

# Placeholders that mean "NOT a real hash" — publishing one is a
# guaranteed install failure, so the gate blocks them outright.
# (Placeholder nghĩa là "KHÔNG phải hash thật" — publish là hỏng install
# chắc chắn, nên cổng chặn thẳng.)
FORBIDDEN_CHECKSUM_PREFIXES = ("PENDING_", "UNAVAILABLE_")

# A real SHA256 hex digest: exactly 64 lowercase hex chars.
# (SHA256 hex thật: đúng 64 ký tự hex thường.)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")

# Workspace version: the first `version = "..."` key in the root
# Cargo.toml (the [workspace.package] section — verified position, line 13).
# (Version workspace: khóa `version = "..."` đầu tiên trong Cargo.toml gốc
# — mục [workspace.package], vị trí đã xác minh ở dòng 13.)
WORKSPACE_VERSION_RE = re.compile(r'^version\s*=\s*"([^"]+)"', re.M)
# Homebrew formula: `version "..."` and `sha256 "..."` slots.
# (Formula homebrew: các slot `version "..."` và `sha256 "..."`.)
HOMEBREW_VERSION_RE = re.compile(r'version\s+"([^"]+)"')
HOMEBREW_SHA256_RE = re.compile(r'sha256\s+"([^"]+)"')


def workspace_version() -> str:
    """Read the workspace version from the root Cargo.toml — the single
    source of truth every manifest must match.
    (Đọc version workspace từ Cargo.toml gốc — nguồn chân lý duy nhất mà
    mọi manifest phải khớp.)"""
    cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = WORKSPACE_VERSION_RE.search(cargo_toml)
    if not match:
        # Fail-closed: no version key → the gate cannot verify → block.
        # (Fail-closed: không có khóa version → cổng không verify được → chặn.)
        print(
            "RELEASE MANIFEST GATE: workspace version not found in "
            f"{ROOT / 'Cargo.toml'} — refusing to pass (fail-closed)",
            file=sys.stderr,
        )
        sys.exit(1)
    return match.group(1)


def check_homebrew(path: Path, expected: str, violations: list) -> None:
    """Check one Homebrew formula: version match + real-sha256-only.
    (Kiểm một formula Homebrew: version khớp + chỉ nhận sha256 thật.)"""
    text = path.read_text(encoding="utf-8")
    version_match = HOMEBREW_VERSION_RE.search(text)
    got = version_match.group(1) if version_match else None
    if got != expected:
        violations.append(f"{path.name}: version '{got}' != workspace '{expected}'")
    for checksum in HOMEBREW_SHA256_RE.findall(text):
        if checksum.startswith(FORBIDDEN_CHECKSUM_PREFIXES):
            violations.append(
                f"{path.name}: checksum placeholder '{checksum}' — publish blocked "
                "(rerun scripts/update-release-hashes.sh after the release assets exist)"
            )
        elif not SHA256_RE.match(checksum):
            violations.append(
                f"{path.name}: checksum '{checksum}' is not a 64-hex SHA256 — publish blocked"
            )


def check_scoop(path: Path, expected: str, violations: list) -> None:
    """Check one Scoop manifest: version match + real-sha256-only.
    (Kiểm một manifest Scoop: version khớp + chỉ nhận sha256 thật.)"""
    data = json.loads(path.read_text(encoding="utf-8"))
    got = data.get("version")
    if got != expected:
        violations.append(f"{path.name}: version '{got}' != workspace '{expected}'")
    architectures = data.get("architecture", {})
    for arch_name, arch in sorted(architectures.items()):
        checksum = arch.get("hash")
        if checksum is None:
            violations.append(f"{path.name}: architecture '{arch_name}' has no hash")
            continue
        if checksum.startswith(FORBIDDEN_CHECKSUM_PREFIXES):
            violations.append(
                f"{path.name}: architecture '{arch_name}' checksum placeholder "
                f"'{checksum}' — publish blocked "
                "(rerun scripts/update-release-hashes.sh after the release assets exist)"
            )
        elif not SHA256_RE.match(checksum):
            violations.append(
                f"{path.name}: architecture '{arch_name}' checksum '{checksum}' "
                "is not a 64-hex SHA256 — publish blocked"
            )


def main() -> int:
    expected = workspace_version()
    violations: list = []
    for path in HOMEBREW_MANIFESTS:
        if not path.is_file():
            # A missing manifest is itself a violation — the release
            # surface must be complete.
            # (Thiếu manifest cũng là vi phạm — mặt release phải đủ.)
            violations.append(f"missing manifest: {path.relative_to(ROOT)}")
            continue
        check_homebrew(path, expected, violations)
    for path in SCOOP_MANIFESTS:
        if not path.is_file():
            violations.append(f"missing manifest: {path.relative_to(ROOT)}")
            continue
        check_scoop(path, expected, violations)

    if violations:
        for v in violations:
            print(f"RELEASE MANIFEST GATE VIOLATION: {v}", file=sys.stderr)
        print(
            f"release manifest gate: {len(violations)} violation(s) — PUBLISH BLOCKED "
            "(fail-closed, Gate E / P0-3)",
            file=sys.stderr,
        )
        return 1
    print(
        f"release manifest gate: {len(HOMEBREW_MANIFESTS) + len(SCOOP_MANIFESTS)} "
        f"manifests verified — version {expected}, all checksums are real SHA256"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
