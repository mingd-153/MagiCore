#!/usr/bin/env python3
"""Adversarial tests for release asset hashing and manifest production."""

import hashlib
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT_DIR = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "release_manifest_builder", SCRIPT_DIR / "release-manifest.py"
)
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)


class ReleaseManifestBuilderTests(unittest.TestCase):
    def test_same_length_mutation_with_mtime_restored_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            asset = Path(temp) / "magicore-1.2.0-linux-x64.tar.gz"
            asset.write_bytes(b"A" * 128)
            before = asset.stat()
            real_sha256 = hashlib.sha256
            mutated = False

            class MutatingDigest:
                def __init__(self):
                    self.inner = real_sha256()

                def update(self, data):
                    nonlocal mutated
                    self.inner.update(data)
                    if not mutated:
                        mutated = True
                        with asset.open("r+b") as handle:
                            handle.seek(0)
                            handle.write(b"B" * 128)
                            handle.flush()
                            os.fsync(handle.fileno())
                        os.utime(asset, ns=(before.st_atime_ns, before.st_mtime_ns))

                def hexdigest(self):
                    return self.inner.hexdigest()

            with mock.patch.object(builder, "HASH_CHUNK_BYTES", 16), mock.patch.object(
                builder.hashlib, "sha256", MutatingDigest
            ):
                with self.assertRaisesRegex(ValueError, "changed while hashing"):
                    builder.hash_release_asset(str(asset))


if __name__ == "__main__":
    unittest.main(verbosity=2)
