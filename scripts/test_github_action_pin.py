#!/usr/bin/env python3
# Tests the GitHub action pin verifier under online and offline responses.
# Kiểm tra verifier SHA của GitHub Actions với phản hồi có mạng và mất mạng.

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts" / "verify_github_action_pin.sh"
EXPECTED = "a" * 40


class GitHubActionPinTests(unittest.TestCase):
    def run_verifier(self, mode: str, *, ci: bool = False) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory(prefix="mgc-action-pin-") as temporary:
            bin_dir = Path(temporary)
            fake_git = bin_dir / "git"
            fake_git.write_text(
                "#!/bin/sh\n"
                "if [ \"$1\" != ls-remote ]; then exit 97; fi\n"
                "case \"${FAKE_GIT_MODE:-offline}\" in\n"
                "  offline) exit 128 ;;\n"
                f"  expected) printf '%s\\trefs/tags/v7.0.1^{{}}\\n' '{EXPECTED}' ;;\n"
                f"  wrong) printf '%s\\trefs/tags/v7.0.1^{{}}\\n' '{'b' * 40}' ;;\n"
                "  *) exit 98 ;;\n"
                "esac\n",
                encoding="utf-8",
            )
            fake_git.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = f"{bin_dir}{os.pathsep}{env['PATH']}"
            env["FAKE_GIT_MODE"] = mode
            if ci:
                env["CI"] = "true"
            else:
                env.pop("CI", None)
                env.pop("GITHUB_ACTIONS", None)
            return subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; verify_pin actions/checkout v7.0.1 "$2"',
                    "test-github-action-pin",
                    str(VERIFIER),
                    EXPECTED,
                ],
                cwd=ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

    def test_matching_upstream_sha_passes(self) -> None:
        result = self.run_verifier("expected")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_mismatching_upstream_sha_fails(self) -> None:
        result = self.run_verifier("wrong")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match upstream commit", result.stderr)

    def test_local_offline_result_is_unverified_not_pass(self) -> None:
        result = self.run_verifier("offline")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("UNVERIFIED:", result.stderr)

    def test_ci_offline_result_fails_closed(self) -> None:
        result = self.run_verifier("offline", ci=True)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("CI cannot reach github.com", result.stderr)


if __name__ == "__main__":
    unittest.main()
