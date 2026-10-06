#!/usr/bin/env python3
"""Regression tests for strict release-manifest signature verification.
Kiểm thử hồi quy xác minh chữ ký manifest phát hành theo chế độ fail-closed.
"""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
import _ed25519

SPEC = importlib.util.spec_from_file_location(
    "verify_release_manifest_signature",
    SCRIPT_DIR / "verify-release-manifest-signature.py",
)
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


class ReleaseManifestSignatureVerification(unittest.TestCase):
    """The release gate rejects unsigned, tampered, and wrongly bound assets."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.assets = Path(self.temp.name)
        self.seed = bytes(range(32))
        self.archives = []
        required = [
            ("magicore", "linux", "x64", "tar.gz"),
            ("magicore", "linux", "arm64", "tar.gz"),
            ("magicore", "macos", "x64", "tar.gz"),
            ("magicore", "macos", "arm64", "tar.gz"),
            ("magicore", "windows", "x64", "zip"),
            ("magicore-web", "linux", "x64", "tar.gz"),
            ("magicore-web", "linux", "arm64", "tar.gz"),
            ("magicore-web", "macos", "x64", "tar.gz"),
            ("magicore-web", "macos", "arm64", "tar.gz"),
            ("magicore-web", "windows", "x64", "zip"),
        ]
        entries = []
        for package, os_name, arch, extension in required:
            archive = self.assets / f"{package}-1.2.0-{os_name}-{arch}.{extension}"
            archive.write_bytes(f"{package}:{os_name}:{arch}".encode())
            self.archives.append(archive)
            entries.append(
                {
                    "package": package,
                    "os": os_name,
                    "arch": arch,
                    "archive": archive.name,
                    "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                    "size": archive.stat().st_size,
                }
            )
        manifest = {
            "version": "1.2.0",
            "artifacts": entries,
        }
        self.manifest_bytes = (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode()
        (self.assets / "manifest.json").write_bytes(self.manifest_bytes)
        signature = _ed25519.sign(self.seed, self.manifest_bytes)
        (self.assets / "manifest.json.sig").write_text(signature.hex() + "\n", encoding="ascii")

    def tearDown(self):
        self.temp.cleanup()

    def run_verifier(self, *archives, tag="v1.2.0", seed=None):
        args = ["verify-release-manifest-signature.py", str(self.assets), tag]
        args.extend(str(path) for path in archives)
        env = {} if seed is None else {"MGC_RELEASE_SIGNING_KEY": seed.hex()}
        stdout = io.StringIO()
        stderr = io.StringIO()
        with mock.patch.dict(os.environ, env, clear=False):
            if seed is None:
                os.environ.pop("MGC_RELEASE_SIGNING_KEY", None)
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                code = verifier.main(args)
        return code, stdout.getvalue(), stderr.getvalue()

    def test_valid_signature_and_archive_binding_pass(self):
        code, output, _error = self.run_verifier(*self.archives, seed=self.seed)
        self.assertEqual(code, 0)
        self.assertIn("manifest signature VALID", output)
        self.assertEqual(output.count("archive binding VALID"), 10)

    def test_missing_required_archive_binding_fails(self):
        code, _output, error = self.run_verifier(*self.archives[:-1], seed=self.seed)
        self.assertEqual(code, 1)
        self.assertIn("every signed archive must be downloaded and verified", error)
        self.assertIn(self.archives[-1].name, error)

    def test_manifest_missing_required_platform_fails(self):
        manifest = json.loads(self.manifest_bytes)
        manifest["artifacts"].pop()
        malformed = json.dumps(manifest, sort_keys=True, indent=2).encode() + b"\n"
        with self.assertRaisesRegex(verifier.VerificationError, "missing required release variants"):
            verifier._parse_manifest(malformed, "v1.2.0")

    def test_manifest_missing_linux_arm64_fails(self):
        manifest = json.loads(self.manifest_bytes)
        manifest["artifacts"] = [
            item for item in manifest["artifacts"]
            if (item["package"], item["os"], item["arch"]) != ("magicore", "linux", "arm64")
        ]
        malformed = json.dumps(manifest, sort_keys=True, indent=2).encode() + b"\n"
        with self.assertRaisesRegex(verifier.VerificationError, "magicore-1.2.0-linux-arm64.tar.gz"):
            verifier._parse_manifest(malformed, "v1.2.0")

    def test_missing_signing_key_fails_closed(self):
        code, _output, error = self.run_verifier(*self.archives)
        self.assertEqual(code, 1)
        self.assertIn("MGC_RELEASE_SIGNING_KEY is required", error)

    def test_missing_signature_fails_closed(self):
        (self.assets / "manifest.json.sig").unlink()
        code, _output, error = self.run_verifier(*self.archives, seed=self.seed)
        self.assertEqual(code, 1)
        self.assertIn("manifest.json.sig is missing", error)

    def test_tampered_manifest_fails_signature_verification(self):
        (self.assets / "manifest.json").write_bytes(self.manifest_bytes + b" ")
        code, _output, error = self.run_verifier(*self.archives, seed=self.seed)
        self.assertEqual(code, 1)
        self.assertIn("manifest signature verification failed", error)

    def test_tag_version_mismatch_fails(self):
        code, _output, error = self.run_verifier(*self.archives, tag="v9.9.9", seed=self.seed)
        self.assertEqual(code, 1)
        self.assertIn("does not match requested tag", error)

    def test_tampered_archive_fails_manifest_binding(self):
        self.archives[0].write_bytes(b"x" * self.archives[0].stat().st_size)
        code, _output, error = self.run_verifier(*self.archives, seed=self.seed)
        self.assertEqual(code, 1)
        self.assertIn("archive digest does not match signed manifest", error)

    def test_archive_outside_download_directory_is_rejected(self):
        outside_dir = self.assets.parent / (self.assets.name + "-outside")
        outside_dir.mkdir()
        outside = outside_dir / self.archives[0].name
        outside.write_bytes(self.archives[0].read_bytes())
        try:
            code, _output, error = self.run_verifier(
                outside, *self.archives[1:], seed=self.seed
            )
            self.assertEqual(code, 1)
            self.assertIn("must be a regular file directly under", error)
        finally:
            outside.unlink()
            outside_dir.rmdir()


if __name__ == "__main__":
    unittest.main(verbosity=2)
