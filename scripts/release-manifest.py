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
import errno
import json
import os
import stat
import sys
import tempfile

HASH_CHUNK_BYTES = 1024 * 1024

KNOWN_OS = {"linux", "macos", "windows"}
KNOWN_ARCH = {"x64", "arm64"}

# Exact release contract: Linux/macOS ship both architectures; Windows ships x64.
# Ma trận phát hành chính xác: Linux/macOS có hai kiến trúc; Windows chỉ có x64.
# A signed manifest missing a required variant is a broken release; fail before signing.
# Manifest thiếu biến thể bắt buộc là bản phát hành lỗi; dừng trước khi ký.
REQUIRED_MATRIX = [
    ("magicore", os_name, arch)
    for os_name, arch in [
        ("linux", "x64"),
        ("linux", "arm64"),
        ("macos", "x64"),
        ("macos", "arm64"),
        ("windows", "x64"),
    ]
] + [
    ("magicore-web", os_name, arch)
    for os_name, arch in [
        ("linux", "x64"),
        ("linux", "arm64"),
        ("macos", "x64"),
        ("macos", "arm64"),
        ("windows", "x64"),
    ]
]


def check_required_matrix(artifacts):
    """Missing/duplicate required (package, os, arch) entries → error
    lines (empty = contract met). Windows ARM64 is outside this release matrix.
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
        digest, size = hash_release_asset(path)
        artifacts.append(
            {
                "package": package,
                "os": os_name,
                "arch": arch,
                "archive": fname,
                "sha256": digest,
                "size": size,
            }
        )
    artifacts.sort(key=lambda a: a["archive"])
    return {"version": version, "artifacts": artifacts}


def hash_release_asset(path):
    """Hash a regular asset through one no-follow descriptor with bounded memory."""
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_BINARY", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        if error.errno == errno.ELOOP:
            raise ValueError(
                f"release asset is not a regular file: {os.path.basename(path)}"
            ) from error
        raise
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            raise ValueError(f"release asset is not a regular file: {os.path.basename(path)}")
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            while True:
                chunk = handle.read(HASH_CHUNK_BYTES)
                if not chunk:
                    break
                digest.update(chunk)
        after = os.fstat(descriptor)
        if (
            before.st_dev,
            before.st_ino,
            before.st_size,
            before.st_mtime_ns,
            before.st_ctime_ns,
        ) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_mtime_ns,
            after.st_ctime_ns,
        ):
            raise ValueError(f"release asset changed while hashing: {os.path.basename(path)}")
        return digest.hexdigest(), after.st_size
    finally:
        os.close(descriptor)


def write_manifest_atomic(assets_dir, manifest):
    """Publish a complete manifest atomically; never leave a partial JSON file."""
    text = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    descriptor, temporary = tempfile.mkstemp(prefix=".manifest-", dir=assets_dir)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, os.path.join(assets_dir, "manifest.json"))
        if hasattr(os, "O_DIRECTORY"):
            directory = os.open(assets_dir, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


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
    try:
        manifest = build_manifest(assets_dir, version)
    except (OSError, ValueError) as error:
        print(f"release-manifest.py: {error}", file=sys.stderr)
        return 2
    if require_matrix:
        errors = check_required_matrix(manifest["artifacts"])
        if errors:
            for e in errors:
                print(f"release-manifest.py: {e}", file=sys.stderr)
            return 2
        print(f"matrix contract met: {len(REQUIRED_MATRIX)} required variants present")
    try:
        write_manifest_atomic(assets_dir, manifest)
    except OSError as error:
        print(f"release-manifest.py: cannot publish manifest atomically: {error}", file=sys.stderr)
        return 2
    print(f"manifest.json: {len(manifest['artifacts'])} artifacts")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
