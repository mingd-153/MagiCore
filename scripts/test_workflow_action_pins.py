#!/usr/bin/env python3
"""Regression tests for immutable GitHub workflow action references."""

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("audit_workflow_action_pins.py")
SPEC = importlib.util.spec_from_file_location("audit_workflow_action_pins", SCRIPT)
assert SPEC and SPEC.loader
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class WorkflowActionPinTests(unittest.TestCase):
    def test_accepts_commit_pinned_remote_and_local_actions(self) -> None:
        findings = AUDIT.audit_text(
            "jobs:\n  check:\n    steps:\n"
            "      - uses: actions/checkout@0123456789abcdef0123456789abcdef01234567\n"
            "      - uses: actions/cache/restore@0123456789abcdef0123456789abcdef01234567\n"
            "      - uses: ./.github/actions/local-check\n",
            Path(".github/workflows/ci.yml"),
        )
        self.assertEqual(findings, [])

    def test_rejects_floating_tag_branch_and_short_sha(self) -> None:
        findings = AUDIT.audit_text(
            "jobs:\n  check:\n    steps:\n"
            "      - uses: actions/checkout@v4\n"
            "      - uses: owner/action@main\n"
            "      - uses: owner/action@1234abcd\n",
            Path("ci.yml"),
        )
        self.assertEqual(len(findings), 3)
        self.assertTrue(all("40-character commit SHA" in item.message for item in findings))

    def test_docker_action_requires_sha256_digest(self) -> None:
        good = "jobs:\n  build:\n    steps:\n      - uses: docker://ghcr.io/acme/tool@sha256:" + "a" * 64 + "\n"
        bad = "jobs:\n  build:\n    steps:\n      - uses: docker://ghcr.io/acme/tool:latest\n"
        self.assertEqual(AUDIT.audit_text(good, Path("good.yml")), [])
        self.assertEqual(len(AUDIT.audit_text(bad, Path("bad.yml"))), 1)

    def test_ignores_comments_and_run_block_contents(self) -> None:
        text = (
            "jobs:\n  check:\n    steps:\n"
            "      - name: script example\n"
            "        run: |\n"
            "          - uses: actions/not-an-action@v1\n"
            "      # - uses: actions/commented@v1\n"
            "      - uses: actions/checkout@0123456789abcdef0123456789abcdef01234567 # pinned\n"
        )
        self.assertEqual(AUDIT.audit_text(text, Path("ci.yml")), [])

    def test_rejects_flow_style_that_could_hide_an_action_reference(self) -> None:
        text = "jobs:\n  check:\n    steps: [{uses: actions/checkout@v4}]\n"
        findings = AUDIT.audit_text(text, Path("ci.yml"))
        self.assertEqual(len(findings), 1)
        self.assertIn("unsupported inline", findings[0].message)

    def test_accepts_quoted_yaml_key_with_a_full_sha(self) -> None:
        text = "jobs:\n  check:\n    steps:\n      - \"uses\": actions/checkout@0123456789abcdef0123456789abcdef01234567\n"
        self.assertEqual(AUDIT.audit_text(text, Path("ci.yml")), [])

    def test_scans_all_yaml_workflows_recursively(self) -> None:
        with tempfile.TemporaryDirectory(prefix="mgc-action-audit-") as temp:
            root = Path(temp)
            (root / "nested").mkdir()
            (root / "ci.yml").write_text("jobs:\n  x:\n    steps:\n      - uses: a/b@v1\n", encoding="utf-8")
            (root / "nested" / "release.yaml").write_text(
                "jobs:\n  x:\n    steps:\n      - uses: a/b@0123456789abcdef0123456789abcdef01234567\n",
                encoding="utf-8",
            )
            findings = AUDIT.audit_workflows(root)
            self.assertEqual(len(findings), 1)
            self.assertEqual(findings[0].path.name, "ci.yml")


if __name__ == "__main__":
    unittest.main()
