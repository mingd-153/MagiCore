#!/usr/bin/env python3
"""Build release-assets/manifest.json binding every archive.

Reads VERSION from the environment (fail-closed with a clear message —
never an unhandled KeyError) and parses archive names from the RIGHT:
``magicore[-web]-<version>-<os>-<arch>.<ext>`` where <version> itself
may contain dashes (e.g. ``1.1.0-rc.9``). The old left-split parser
silently dropped every RC asset; this one cannot.

Usage: VERSION=1.1.0-rc.9 python3 scripts/release-manifest.py <assets-dir>
"""

import hashlib
import json
import os
import sys

KNOWN_OS = {"linux", "macos", "windows"}
KNOWN_ARCH = {"x64", "arm64"}

# Exact release contract (P0: mirrors pre-publish-verification — macOS
# ships BOTH arm64 and x64; linux/windows ship x64 in RC, arm64 entries
# are bonus-only). A SIGNED manifest missing a required variant is a
# broken release: correct format, wrong content — fail BEFORE signing.
# (Ma trận phát hành chính xác — thiếu biến thể bắt buộc thì fail trước ký.)
REQUIRED_MATRIX = [
    ("magicore", os_name, arch)
    for os_name, arch in [
        ("linux", "x64"),
        ("macos", "x64"),
        ("macos", "arm64"),
        ("windows", "x64"),
    ]
] + [
    ("magicore-web", os_name, arch)
    for os_name, arch in [
        ("linux", "x64"),
        ("macos", "x64"),
        ("macos", "arm64"),
        ("windows", "x64"),
    ]
]


def check_required_matrix(artifacts):
    """Missing/duplicate required (package, os, arch) entries → error
    lines (empty = contract met). Bonus entries (linux/windows arm64)
    never fail.
    (Thiếu/trùng biến thể bắt buộc → dòng lỗi.)"""
    errors = []
    seen = {}
    for a in artifacts:
        key = (a["package"], a["os"], a["arch"])
        seen[key] = seen.get(key, 0) + 1
    for key, count in sorted(seen.items()):
        if count > 1:
            errors.append(f"duplicate manifest entry for {key[0]}-{key[1]}-{key[2]}")
    have = set(seen)
    for package, os_name, arch in REQUIRED_MATRIX:
        if (package, os_name, arch) not in have:
            errors.append(f"missing required release asset: {package}-<version>-{os_name}-{arch}")
    return errors


def parse_archive_name(fname, version):
    """Return (package, os, arch) or None when the name is not a
    version-matching release archive."""
    if fname.endswith(".tar.gz"):
        stem, ext_ok = fname[: -len(".tar.gz")], True
    elif fname.endswith(".zip"):
        stem, ext_ok = fname[: -len(".zip")], True
    else:
        return None
    if not ext_ok:
        return None
    parts = stem.split("-")
    if not parts or parts[0] != "magicore":
        return None
    if len(parts) > 1 and parts[1] == "web":
        package, rest = "magicore-web", parts[2:]
    else:
        package, rest = "magicore", parts[1:]
    # os + arch are the LAST two segments; everything between the
    # package prefix and them is the version (dashes allowed).
    if len(rest) < 3:
        return None
    os_name, arch = rest[-2], rest[-1]
    file_version = "-".join(rest[:-2])
    if os_name not in KNOWN_OS or arch not in KNOWN_ARCH:
        return None
    if file_version != version:
        return None
    return package, os_name, arch


def build_manifest(assets_dir, version):
    artifacts = []
    for fname in sorted(os.listdir(assets_dir)):
        parsed = parse_archive_name(fname, version)
        if parsed is None:
            continue
        package, os_name, arch = parsed
        path = os.path.join(assets_dir, fname)
        if not os.path.isfile(path):
            continue
        with open(path, "rb") as fh:
            digest = hashlib.sha256(fh.read()).hexdigest()
        artifacts.append(
            {
                "package": package,
                "os": os_name,
                "arch": arch,
                "archive": fname,
                "sha256": digest,
                "size": os.path.getsize(path),
            }
        )
    artifacts.sort(key=lambda a: a["archive"])
    return {"version": version, "artifacts": artifacts}


def main(argv):
    require_matrix = "--require-matrix" in argv
    argv = [a for a in argv if a != "--require-matrix"]
    if len(argv) != 2:
        print("usage: release-manifest.py [--require-matrix] <assets-dir>", file=sys.stderr)
        return 2
    version = os.environ.get("VERSION", "").strip()
    if not version:
        print(
            "release-manifest.py: VERSION is not set or empty — "
            "export VERSION=<release-version> before running",
            file=sys.stderr,
        )
        return 2
    assets_dir = argv[1]
    manifest = build_manifest(assets_dir, version)
    if require_matrix:
        errors = check_required_matrix(manifest["artifacts"])
        if errors:
            for e in errors:
                print(f"release-manifest.py: {e}", file=sys.stderr)
            return 2
        print(f"matrix contract met: {len(REQUIRED_MATRIX)} required variants present")
    text = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    with open(os.path.join(assets_dir, "manifest.json"), "w") as fh:
        fh.write(text)
    print(f"manifest.json: {len(manifest['artifacts'])} artifacts")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
