#!/usr/bin/env python3
"""Mutation-writer call manifest gate (P1-2).

Every manifest/lock mutation in production code MUST flow through the
transaction gateway (cli/src/commands/core/shared.rs journaled paths).
This gate scans for DIRECT mutation-writer calls and fails CI when a
call appears in a function that is not allowlisted below — a new
parse → edit → write path that bypasses lock/recovery/journal/rollback
(like the web-lane adapter.add bypass, fixed after a race E2E caught it
live) must be justified and listed here, never silent.

HOW TO EXTEND (reviews must demand this): add a `path :: fn` line with
a one-line justification comment above it, proving the new caller either
(a) runs under the gateway (holds &ProjectWriteLock + stages/recovers),
or (b) is an adapter-internal writer implementation (the engine itself,
not an orchestrator bypass). Test/bench files are excluded.

Cổng manifest lệnh ghi mutation (P1-2): mọi mutation manifest/lock ở
production phải qua gateway transaction — lệnh gọi trực tiếp ngoài danh
sách cho phép làm CI fail.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCAN_DIRS = ["cli/src", "adapters", "core"]
EXT = ".rs"


def is_test_path(rel, basename):
    return (
        "/test/" in "/" + rel + "/"
        or "/tests/" in "/" + rel + "/"
        or "/benches/" in "/" + rel + "/"
        or basename.startswith("test_")
        or basename.endswith("_test.rs")
    )

# (file_suffix, function_name) pairs allowed to call mutation writers.
# Adapter-internal writer implementations (the engine) + gateway-routed
# orchestrator paths (lock + journal + rollback). Everything else fails.
# Names are impl-qualified where the call sits in a method.
ALLOWLIST = {
    # --- gateway machinery itself (owns lock + journal + rollback) ---
    ("cli/src/commands/core/shared.rs", "journaled_step"),
    ("cli/src/commands/core/shared.rs", "restore_mutation_snapshot"),
    ("cli/src/commands/core/shared.rs", "adopt_legacy_journal"),
    # --- gateway-routed mutation tails (run under the caller's guard) ---
    ("cli/src/commands/core/shared.rs", "add"),
    ("cli/src/commands/core/shared.rs", "remove"),
    ("cli/src/commands/core/shared.rs", "native_update_locked"),
    ("cli/src/commands/core/shared.rs", "install_with_adapter_locked"),
    ("cli/src/commands/install.rs", "install_into_root"),
    ("cli/src/commands/install.rs", "install_resolve_and_materialize"),
    # --- migrate holds the writer guard + refuses pending journals ---
    ("cli/src/commands/migrate.rs", "run_lock"),
    # --- adapter-internal writer implementations (the engine, not a bypass) ---
    ("adapters/web/src/lib.rs", "DependencyResolver::add"),
    ("adapters/cloud/src/adapter.rs", "LockfileProvider::write_manifest"),
    ("adapters/lib/src/adapter.rs", "LockfileProvider::write_manifest"),
    ("core/crates/mgc-adapter-base/src/lib.rs", "base_add"),
    ("core/crates/mgc-adapter-base/src/lib.rs", "base_remove"),
}

PATTERNS = [
    re.compile(r"\.write_manifest\s*\("),
    re.compile(r"\batomic_write_locked\s*\("),
]

# The definitions themselves are not calls.
DEF_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(?:atomic_write_locked|write_manifest)\b")

FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")
IMPL_RE = re.compile(r"^\s*impl(?:\s*<[^>]*>)?\s+(\w+)")


def strip_noise_lines(lines):
    """Strip // comments and string/char contents FILE-WISE (state
    carries across lines) so multi-line strings cannot corrupt brace
    counting — e.g. a `{` inside a continued format string."""
    cleaned = []
    in_str, in_char = False, False
    for line in lines:
        out = []
        i, n = 0, len(line)
        while i < n:
            char = line[i]
            if in_str:
                if char == "\\":
                    i += 2
                    continue
                if char == '"':
                    in_str = False
                i += 1
                continue
            if in_char:
                if char == "\\":
                    i += 2
                    continue
                if char == "'":
                    in_char = False
                i += 1
                continue
            if char == "/" and i + 1 < n and line[i + 1] == "/":
                break
            if char == '"':
                in_str = True
                i += 1
                continue
            if char == "'":
                in_char = True
                i += 1
                continue
            out.append(char)
            i += 1
        cleaned.append("".join(out))
    return cleaned


def enclosing_fn(path, lineno, lines):
    """Forward scan (correct brace direction): per line, pop closed
    scopes first, then record fn/impl openings, then update depth."""
    stack = []  # [kind, name, depth_before, opened]
    depth = 0
    cleaned = strip_noise_lines(lines)
    for i in range(lineno - 1):
        line = lines[i]
        while stack and stack[-1][3] and stack[-1][2] >= depth:
            stack.pop()
        m = IMPL_RE.match(line)
        if m:
            stack.append(["impl", m.group(1), depth, False])
        else:
            m = FN_RE.match(line)
            if m:
                stack.append(["fn", m.group(1), depth, False])
        clean = cleaned[i]
        depth += clean.count("{") - clean.count("}")
        for scope in stack:
            if scope[2] < depth:
                scope[3] = True
    fn_name = next((scope[1] for scope in reversed(stack) if scope[0] == "fn"), None)
    if fn_name is None:
        return None
    impl_name = next((scope[1] for scope in reversed(stack) if scope[0] == "impl"), None)
    return f"{impl_name}::{fn_name}" if impl_name else fn_name


def main():
    violations = []
    for scan in SCAN_DIRS:
        for dirpath, _dirnames, filenames in os.walk(os.path.join(ROOT, scan)):
            for filename in filenames:
                if not filename.endswith(EXT):
                    continue
                full = os.path.join(dirpath, filename)
                rel = os.path.relpath(full, ROOT)
                if is_test_path(rel, filename):
                    continue
                with open(full, encoding="utf-8", errors="replace") as handle:
                    lines = handle.readlines()
                for idx, line in enumerate(lines, start=1):
                    if not any(p.search(line) for p in PATTERNS):
                        continue
                    if DEF_RE.match(line):
                        continue
                    qualified = enclosing_fn(rel, idx, lines)
                    if (rel, qualified) not in ALLOWLIST:
                        violations.append(f"{rel}:{idx}: {line.strip()} (fn {qualified})")
    if violations:
        print("MUTATION-CALL MANIFEST VIOLATIONS (add justification to")
        print("scripts/audit_mutation_calls.py ALLOWLIST or route via gateway):")
        for violation in violations:
            print("  " + violation)
        return 1
    print(f"OK: mutation-writer calls confined to {len(ALLOWLIST)} allowlisted sites")
    return 0


if __name__ == "__main__":
    sys.exit(main())
