#!/usr/bin/env python3
"""Verify generated package-manager URLs and hashes against release assets.

This is the strict counterpart to update-manifests.sh --verify-only. It checks
each URL/hash pair, not merely whether a digest happens to occur in a file.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

RELEASE_BASE = "https://github.com/mingd-153/MagiCore/releases/download/v"
FORMULA_ENTRY_RE = re.compile(
    r'^\s*url\s+"([^"]+)"\s*\n\s*sha256\s+"([^"]+)"\s*$', re.M
)
FORMULA_URL_RE = re.compile(r'^\s*url\s+"([^"]+)"', re.M)
FORMULA_VERSION_RE = re.compile(r'^\s*version\s+"([^"]+)"', re.M)


def fail(message: str) -> None:
    print(f"manifest verification failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_entry(artifacts: Path, version: str, filename: str) -> tuple[str, str]:
    artifact = artifacts / filename
    if not artifact.is_file():
        fail(f"missing release artifact: {artifact}")
    return f"{RELEASE_BASE}{version}/{filename}", sha256_file(artifact)


def check_formula(path: Path, version: str, expected: list[tuple[str, str]]) -> None:
    if not path.is_file():
        fail(f"missing Homebrew formula: {path}")
    text = path.read_text(encoding="utf-8")
    version_match = FORMULA_VERSION_RE.search(text)
    if not version_match or version_match.group(1) != version:
        actual = version_match.group(1) if version_match else "missing"
        fail(f"{path.name}: formula version {actual!r}, expected {version!r}")

    urls = FORMULA_URL_RE.findall(text)
    entries = FORMULA_ENTRY_RE.findall(text)
    if len(urls) != len(entries):
        fail(
            f"{path.name}: found {len(urls)} URL(s) but only {len(entries)} adjacent URL/SHA256 pair(s)"
        )
    if entries != expected:
        for index, (actual, wanted) in enumerate(zip(entries, expected), start=1):
            if actual != wanted:
                fail(
                    f"{path.name}: entry {index} differs; expected URL/hash for the matching release artifact"
                )
        fail(f"{path.name}: found {len(entries)} URL/hash entries, expected {len(expected)}")


def check_scoop(
    path: Path, version: str, expected_url: str, expected_hash: str
) -> None:
    if not path.is_file():
        fail(f"missing Scoop manifest: {path}")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"{path.name}: invalid JSON ({error})")
    if data.get("version") != version:
        fail(f"{path.name}: version {data.get('version')!r}, expected {version!r}")
    architecture = data.get("architecture")
    entry = architecture.get("64bit") if isinstance(architecture, dict) else None
    if not isinstance(entry, dict):
        fail(f"{path.name}: missing architecture.64bit entry")
    if set(architecture) != {"64bit"}:
        extras = sorted(set(architecture) - {"64bit"})
        missing = sorted({"64bit"} - set(architecture))
        fail(
            f"{path.name}: architecture set is not backed by this release matrix "
            f"(unexpected={extras}, missing={missing})"
        )
    if entry.get("url") != expected_url:
        fail(f"{path.name}: architecture.64bit URL does not match its release artifact")
    if entry.get("hash") != expected_hash:
        fail(f"{path.name}: architecture.64bit SHA256 does not match its release artifact")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    args = parser.parse_args()

    version = args.version.removeprefix("v")
    artifacts = args.artifacts
    homebrew = args.repo_root / "packaging" / "homebrew"
    scoop = args.repo_root / "packaging" / "scoop"

    core_brew = [
        expected_entry(artifacts, version, f"magicore-{version}-macos-arm64.tar.gz"),
        expected_entry(artifacts, version, f"magicore-{version}-macos-x64.tar.gz"),
        expected_entry(artifacts, version, f"magicore-{version}-linux-x64.tar.gz"),
    ]
    web_brew = [
        expected_entry(artifacts, version, f"magicore-web-{version}-macos-arm64.tar.gz"),
        expected_entry(artifacts, version, f"magicore-web-{version}-macos-x64.tar.gz"),
        expected_entry(artifacts, version, f"magicore-web-{version}-linux-x64.tar.gz"),
    ]
    check_formula(homebrew / "magicore.rb", version, core_brew)
    check_formula(homebrew / "magicore-web.rb", version, web_brew)

    scoop_core = expected_entry(artifacts, version, f"magicore-{version}-windows-x64.zip")
    scoop_web = expected_entry(artifacts, version, f"magicore-web-{version}-windows-x64.zip")
    check_scoop(scoop / "magicore.json", version, *scoop_core)
    check_scoop(scoop / "magicore-web.json", version, *scoop_web)

    print("all 8 generated manifest URL/SHA256 entries match their release assets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
