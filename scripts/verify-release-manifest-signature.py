#!/usr/bin/env python3
"""Verify a signed release manifest and downloaded archive bindings.
Xác minh manifest phát hành đã ký và liên kết hash với archive đã tải.
"""

import hashlib
import json
import os
import re
import stat
import sys

MAX_MANIFEST_BYTES = 4 * 1024 * 1024
MAX_SIGNATURE_BYTES = 4096
HASH_CHUNK_BYTES = 1024 * 1024
SHA256_HEX = re.compile(r"^[0-9a-f]{64}$")
SIGNATURE_HEX = re.compile(r"^[0-9a-f]{128}$")
REQUIRED_VARIANTS = (
    ("magicore", "linux", "x64"),
    ("magicore", "macos", "x64"),
    ("magicore", "macos", "arm64"),
    ("magicore", "windows", "x64"),
    ("magicore-web", "linux", "x64"),
    ("magicore-web", "macos", "x64"),
    ("magicore-web", "macos", "arm64"),
    ("magicore-web", "windows", "x64"),
)


class VerificationError(Exception):
    """Expected invalid or incomplete release evidence."""


def _read_regular_file(path, limit, label):
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise VerificationError(f"{label} is not a regular file")
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            content = handle.read(limit + 1)
        if len(content) > limit:
            raise VerificationError(f"{label} exceeds the {limit}-byte verification limit")
        return content
    finally:
        os.close(descriptor)


def _reject_duplicate_json_keys(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise VerificationError(f"manifest contains duplicate JSON key: {key}")
        value[key] = item
    return value


def _parse_manifest(manifest_bytes, expected_tag):
    try:
        manifest = json.loads(
            manifest_bytes.decode("utf-8"),
            object_pairs_hook=_reject_duplicate_json_keys,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise VerificationError(f"manifest is not valid UTF-8 JSON: {error}") from error

    expected_version = expected_tag[1:] if expected_tag.startswith("v") else expected_tag
    if not isinstance(manifest, dict) or manifest.get("version") != expected_version:
        actual = manifest.get("version") if isinstance(manifest, dict) else "<invalid>"
        raise VerificationError(
            f"manifest version {actual!r} does not match requested tag {expected_tag!r}"
        )
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        raise VerificationError("manifest has no artifact entries")

    by_name = {}
    identities = set()
    for index, artifact in enumerate(artifacts):
        if not isinstance(artifact, dict):
            raise VerificationError(f"manifest artifact {index} is not an object")
        name = artifact.get("archive")
        digest = artifact.get("sha256")
        size = artifact.get("size")
        if not isinstance(name, str) or not name or os.path.basename(name) != name:
            raise VerificationError(f"manifest artifact {index} has an unsafe archive name")
        if name in by_name:
            raise VerificationError(f"manifest has duplicate archive entry: {name}")
        if not isinstance(digest, str) or not SHA256_HEX.fullmatch(digest):
            raise VerificationError(f"manifest artifact {name} has an invalid SHA-256")
        if isinstance(size, bool) or not isinstance(size, int) or size < 0:
            raise VerificationError(f"manifest artifact {name} has an invalid size")
        for field in ("package", "os", "arch"):
            if not isinstance(artifact.get(field), str) or not artifact[field]:
                raise VerificationError(f"manifest artifact {name} has an invalid {field}")
        identity = (artifact["package"], artifact["os"], artifact["arch"])
        expected_name = _archive_name(identity, expected_version)
        if expected_name != name:
            raise VerificationError(
                f"manifest artifact identity does not match archive name: {name}"
            )
        if identity in identities:
            raise VerificationError(f"manifest has duplicate platform variant: {identity}")
        identities.add(identity)
        by_name[name] = artifact
    missing = [
        _archive_name(identity, expected_version)
        for identity in REQUIRED_VARIANTS
        if identity not in identities
    ]
    if missing:
        raise VerificationError(
            "manifest is missing required release variants: " + ", ".join(missing)
        )
    return by_name


def _archive_name(identity, version):
    package, os_name, arch = identity
    if package not in ("magicore", "magicore-web"):
        raise VerificationError(f"manifest has unsupported package identity: {package}")
    if os_name not in ("linux", "macos", "windows") or arch not in ("x64", "arm64"):
        raise VerificationError(f"manifest has unsupported platform identity: {identity}")
    extension = "zip" if os_name == "windows" else "tar.gz"
    return f"{package}-{version}-{os_name}-{arch}.{extension}"


def _verify_archive(path, assets_dir, artifacts):
    archive_path = os.path.abspath(path)
    assets_root = os.path.abspath(assets_dir)
    if os.path.dirname(archive_path) != assets_root:
        raise VerificationError(
            f"archive must be a regular file directly under the download directory: {path}"
        )
    name = os.path.basename(archive_path)
    entry = artifacts.get(name)
    if entry is None:
        raise VerificationError(f"signed manifest does not bind downloaded archive: {name}")

    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(archive_path, flags)
    try:
        file_stat = os.fstat(descriptor)
        if not stat.S_ISREG(file_stat.st_mode):
            raise VerificationError(f"archive is not a regular file: {name}")
        if file_stat.st_size != entry["size"]:
            raise VerificationError(
                f"archive size does not match signed manifest: {name}"
            )
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            while True:
                chunk = handle.read(HASH_CHUNK_BYTES)
                if not chunk:
                    break
                digest.update(chunk)
        if digest.hexdigest() != entry["sha256"]:
            raise VerificationError(
                f"archive digest does not match signed manifest: {name}"
            )
    finally:
        os.close(descriptor)
    return name


def main(argv):
    if len(argv) < 4:
        print(
            "usage: verify-release-manifest-signature.py <assets-dir> <tag> <archive> [archive ...]",
            file=sys.stderr,
        )
        return 2

    seed_hex = os.environ.get("MGC_RELEASE_SIGNING_KEY", "").strip()
    if not re.fullmatch(r"[0-9a-fA-F]{64}", seed_hex):
        print(
            "verification failed: MGC_RELEASE_SIGNING_KEY is required and must be 64 hex characters",
            file=sys.stderr,
        )
        return 1

    assets_dir, expected_tag, *archive_paths = argv[1:]
    manifest_path = os.path.join(assets_dir, "manifest.json")
    signature_path = os.path.join(assets_dir, "manifest.json.sig")
    try:
        if not os.path.lexists(signature_path):
            raise VerificationError("manifest.json.sig is missing")
        manifest_bytes = _read_regular_file(
            manifest_path, MAX_MANIFEST_BYTES, "manifest.json"
        )
        signature_text = _read_regular_file(
            signature_path, MAX_SIGNATURE_BYTES, "manifest.json.sig"
        ).decode("ascii").strip()
        if not SIGNATURE_HEX.fullmatch(signature_text):
            raise VerificationError("manifest.json.sig is missing or malformed")

        artifacts = _parse_manifest(manifest_bytes, expected_tag)
        supplied_names = [os.path.basename(path) for path in archive_paths]
        if len(supplied_names) != len(set(supplied_names)):
            raise VerificationError("duplicate archive argument was supplied")
        if set(supplied_names) != set(artifacts):
            missing = sorted(set(artifacts) - set(supplied_names))
            unexpected = sorted(set(supplied_names) - set(artifacts))
            details = []
            if missing:
                details.append("not downloaded: " + ", ".join(missing))
            if unexpected:
                details.append("not in signed manifest: " + ", ".join(unexpected))
            raise VerificationError(
                "every signed archive must be downloaded and verified (" + "; ".join(details) + ")"
            )
        script_dir = os.path.dirname(os.path.abspath(__file__))
        if script_dir not in sys.path:
            sys.path.insert(0, script_dir)
        import _ed25519

        seed = bytes.fromhex(seed_hex)
        public_key = _ed25519.public_key(seed)
        if not _ed25519.verify(public_key, manifest_bytes, bytes.fromhex(signature_text)):
            raise VerificationError("manifest signature verification failed")

        print(
            f"manifest signature VALID (Ed25519, version={expected_tag}, artifacts={len(artifacts)})"
        )
        for archive_path in archive_paths:
            name = _verify_archive(archive_path, assets_dir, artifacts)
            print(f"archive binding VALID (signed SHA-256 + size): {name}")
    except (OSError, UnicodeDecodeError, ValueError, VerificationError) as error:
        print(f"verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
