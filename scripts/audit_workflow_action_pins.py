#!/usr/bin/env python3
"""Reject mutable or unpinned GitHub Actions in workflow YAML.

Từ chối GitHub Action có ref thay đổi được hoặc không được pin trong workflow YAML.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


_USES_KEY = re.compile(r"^(?P<indent>\s*)(?:-\s*)?(?:uses|'uses'|\"uses\")\s*:\s*(?P<value>.*?)\s*$")
_USES_TOKEN = re.compile(r"\buses\s*:")
_SCALAR_KEY = re.compile(r"^\s*[^#][^:]*:\s*[|>][+-]?[1-9]?\s*(?:#.*)?$")
_REMOTE_ACTION = re.compile(
    r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*@([0-9a-f]{40})(?:/[^\s]*)?$"
)
_DOCKER_ACTION = re.compile(r"^docker://[^\s@]+@sha256:[0-9a-f]{64}$")


class Finding:
    def __init__(self, path: Path, line: int, message: str) -> None:
        self.path = path
        self.line = line
        self.message = message

    def __str__(self) -> str:
        return f"{self.path}:{self.line}: {self.message}"


def _strip_comment(value: str) -> str:
    quote = None
    escaped = False
    for index, char in enumerate(value):
        if escaped:
            escaped = False
        elif char == "\\" and quote == '"':
            escaped = True
        elif quote is not None:
            if char == quote:
                quote = None
        elif char in ("'", '"'):
            quote = char
        elif char == "#" and (index == 0 or value[index - 1].isspace()):
            return value[:index].rstrip()
    return value.strip()


def _valid_action_reference(reference: str) -> bool:
    if reference.startswith("./"):
        return all(part not in ("", ".", "..") for part in reference[2:].split("/"))
    if reference.startswith("docker://"):
        return _DOCKER_ACTION.fullmatch(reference) is not None
    return _REMOTE_ACTION.fullmatch(reference) is not None


def audit_text(text: str, path: Path) -> list[Finding]:
    findings: list[Finding] = []
    block_scalar_indent: int | None = None

    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if block_scalar_indent is not None:
            if not stripped or len(line) - len(stripped) > block_scalar_indent:
                continue
            block_scalar_indent = None

        if not stripped or stripped.startswith("#"):
            continue

        if _SCALAR_KEY.match(line):
            block_scalar_indent = len(line) - len(stripped)
            continue

        match = _USES_KEY.match(line)
        if not match:
            # Reject flow/inline YAML forms we do not parse instead of letting
            # an action reference evade the line-oriented workflow audit.
            # Từ chối YAML flow/inline chưa phân tích để không lọt action reference.
            if _USES_TOKEN.search(_strip_comment(line)):
                findings.append(Finding(path, line_number, "unsupported inline 'uses:' syntax; write it as a YAML mapping key"))
            continue

        reference = _strip_comment(match.group("value"))
        if len(reference) >= 2 and reference[0] == reference[-1] and reference[0] in ("'", '"'):
            reference = reference[1:-1]
        if not reference or not _valid_action_reference(reference):
            findings.append(
                Finding(
                    path,
                    line_number,
                    "action reference must be a local ./ path, a full 40-character commit SHA, "
                    "or a Docker image pinned by sha256 digest",
                )
            )

    return findings


def audit_workflows(root: Path) -> list[Finding]:
    if not root.is_dir():
        return [Finding(root, 0, "workflow directory does not exist")]
    files = sorted(path for path in root.rglob("*") if path.is_file() and path.suffix in (".yml", ".yaml"))
    if not files:
        return [Finding(root, 0, "no YAML workflow files found")]

    findings: list[Finding] = []
    for path in files:
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            findings.append(Finding(path, 0, f"cannot read workflow: {error}"))
            continue
        findings.extend(audit_text(text, path.relative_to(root)))
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "root",
        nargs="?",
        type=Path,
        default=Path(__file__).resolve().parents[1] / ".github" / "workflows",
        help="directory containing GitHub workflow YAML files",
    )
    args = parser.parse_args()
    findings = audit_workflows(args.root)
    if findings:
        for finding in findings:
            print(f"FAIL: {finding}", file=sys.stderr)
        return 1
    print(f"PASS: all YAML workflows under {args.root} use immutable action references")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
