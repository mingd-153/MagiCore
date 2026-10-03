#!/usr/bin/env python3
"""Regression tests for audit-matrix source provenance.
Kiểm thử hồi quy provenance nguồn của audit matrix.
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

import audit_capability_matrix


def committed_repo() -> tuple[tempfile.TemporaryDirectory, Path]:
    temp = tempfile.TemporaryDirectory()
    path = Path(temp.name)
    subprocess.run(["git", "init", "--quiet", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.email", "test@example.invalid"], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.name", "Test"], check=True)
    (path / "tracked.txt").write_text("clean\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(path), "add", "tracked.txt"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "--quiet", "-m", "base"], check=True)
    return temp, path


class SourceRevisionStateTests(unittest.TestCase):
    def test_clean_commit_is_recorded_as_evidence_revision(self) -> None:
        temp, repo = committed_repo()
        try:
            state = audit_capability_matrix.source_revision_state(repo)
            self.assertTrue(state["working_tree_clean"])
            self.assertEqual(state["commit"], state["base_commit"])
            self.assertRegex(state["commit"], r"^[0-9a-f]{40}$")
        finally:
            temp.cleanup()

    def test_dirty_tracked_file_is_not_attributed_to_head(self) -> None:
        temp, repo = committed_repo()
        try:
            (repo / "tracked.txt").write_text("changed\n", encoding="utf-8")
            state = audit_capability_matrix.source_revision_state(repo)
            self.assertFalse(state["working_tree_clean"])
            self.assertIsNone(state["commit"])
            self.assertRegex(state["base_commit"], r"^[0-9a-f]{40}$")
        finally:
            temp.cleanup()

    def test_untracked_file_makes_source_unverified(self) -> None:
        temp, repo = committed_repo()
        try:
            (repo / "untracked.txt").write_text("new\n", encoding="utf-8")
            state = audit_capability_matrix.source_revision_state(repo)
            self.assertFalse(state["working_tree_clean"])
            self.assertIsNone(state["commit"])
        finally:
            temp.cleanup()

    def test_non_git_directory_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            state = audit_capability_matrix.source_revision_state(Path(directory))
        self.assertFalse(state["working_tree_clean"])
        self.assertIsNone(state["commit"])
        self.assertIsNone(state["base_commit"])


if __name__ == "__main__":
    unittest.main()
