#!/usr/bin/env python3
"""Regression tests for release-manifest completeness and checksum binding.
Kiểm tra hồi quy tính đầy đủ và ràng buộc checksum của manifest phát hành.
"""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("verify_release_manifests.py")
SPEC = importlib.util.spec_from_file_location("verify_release_manifests", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class HomebrewManifestCompleteness(unittest.TestCase):
    def check(self, text: str) -> list[str]:
        with tempfile.TemporaryDirectory() as temp_dir:
            formula = Path(temp_dir) / "magicore.rb"
            formula.write_text(text, encoding="utf-8")
            violations: list[str] = []
            VERIFY.check_homebrew(formula, "1.2.0", violations)
            return violations

    def test_missing_checksum_for_download_is_rejected(self) -> None:
        violations = self.check(
            'version "1.2.0"\nurl "https://example.invalid/mgc.tar.gz"\n'
        )
        self.assertTrue(any("checksum" in item.lower() for item in violations))

    def test_missing_download_url_is_rejected(self) -> None:
        violations = self.check('version "1.2.0"\n')
        self.assertTrue(any("url" in item.lower() for item in violations))

    def test_each_download_requires_its_own_checksum(self) -> None:
        violations = self.check(
            'version "1.2.0"\n'
            'url "https://example.invalid/mgc-arm.tar.gz"\n'
            f'sha256 "{"a" * 64}"\n'
            'url "https://example.invalid/mgc-x64.tar.gz"\n'
        )
        self.assertTrue(
            any("requires exactly one checksum" in item.lower() for item in violations)
        )

    def test_complete_download_and_checksum_pair_passes(self) -> None:
        violations = self.check(
            'version "1.2.0"\n'
            'url "https://example.invalid/mgc.tar.gz"\n'
            f'sha256 "{"a" * 64}"\n'
        )
        self.assertEqual(violations, [])


class ScoopManifestCompleteness(unittest.TestCase):
    def check(self, payload: dict) -> list[str]:
        with tempfile.TemporaryDirectory() as temp_dir:
            manifest = Path(temp_dir) / "magicore.json"
            manifest.write_text(json.dumps(payload), encoding="utf-8")
            violations: list[str] = []
            VERIFY.check_scoop(manifest, "1.2.0", violations)
            return violations

    def test_missing_architecture_entries_are_rejected(self) -> None:
        violations = self.check({"version": "1.2.0"})
        self.assertTrue(any("architecture" in item.lower() for item in violations))

    def test_architecture_without_download_url_is_rejected(self) -> None:
        violations = self.check(
            {
                "version": "1.2.0",
                "architecture": {"64bit": {"hash": "a" * 64}},
            }
        )
        self.assertTrue(any("url" in item.lower() for item in violations))

    def test_complete_architecture_entry_passes(self) -> None:
        violations = self.check(
            {
                "version": "1.2.0",
                "architecture": {
                    "64bit": {
                        "url": "https://example.invalid/mgc.zip",
                        "hash": "a" * 64,
                    }
                },
            }
        )
        self.assertEqual(violations, [])


if __name__ == "__main__":
    unittest.main()
