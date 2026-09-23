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

# {(file, qualified-fn): ({allowed sink categories}, justification)}.
# A listed function gaining a NEW sink category fails the gate until the
# manifest records why that sink is safe there.
ALLOWLIST = {
    # --- gateway machinery itself (owns lock + journal + rollback) ---
    ("cli/src/commands/core/shared.rs", "restore_mutation_snapshot"): (
        {"manifest-write", "lock-write"}, "verified restore path",
    ),
    # --- gateway-routed mutation tails (run under the caller's guard) ---
    ("cli/src/commands/core/shared.rs", "add"): (
        {"manifest-write"}, "gateway entry, journaled tail",
    ),
    ("cli/src/commands/core/shared.rs", "remove"): (
        {"manifest-write"}, "gateway entry, journaled tail",
    ),
    ("cli/src/commands/core/shared.rs", "native_update_locked"): (
        {"manifest-write"}, "gateway entry (via update/native_update), journaled tail",
    ),
    ("cli/src/commands/install.rs", "install_into_root"): (
        {"manifest-write"}, "gateway entry, journaled tail",
    ),
    ("cli/src/commands/install.rs", "install_resolve_and_materialize"): (
        set(), "tail only; manifest mutation lives in install_into_root",
    ),
    # --- migrate holds the writer guard + refuses pending journals ---
    ("cli/src/commands/migrate.rs", "run_lock"): (
        {"lock-write"}, "atomic lock rewrite under held guard",
    ),
    # --- adapter-internal writer implementations (the engine, not a bypass) ---
    ("adapters/web/src/lib.rs", "DependencyResolver::add"): (
        {"manifest-write", "fs-write"}, "engine-owned manifest writer",
    ),
    ("adapters/cloud/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the web writer",
    ),
    ("adapters/lib/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the web writer",
    ),
    ("core/crates/mgc-adapter-base/src/lib.rs", "base_add"): (
        {"manifest-write"}, "engine default add implementation",
    ),
    ("core/crates/mgc-adapter-base/src/lib.rs", "base_remove"): (
        {"manifest-write"}, "engine default remove implementation",
    ),
    # --- idempotent convergent edit (lock + atomic, re-runnable) ---
    ("cli/src/commands/core/shared.rs", "game_hook_optimizer_dep"): (
        {"fs-write"}, "insert-if-missing under writer lock, atomic write",
    ),
    # --- explicit delegated/toolchain paths (gateway lock held by the
    # --- caller; the TOOL owns the file, mgc never journals it) ---
    ("cli/src/commands/core/shared.rs", "tool_add_real"): (
        {"adapter-mutation"}, "sole-writer protocol: real toolchain add + re-read verify",
    ),
    ("cli/src/commands/core/shared.rs", "tool_remove_real"): (
        {"adapter-mutation"}, "sole-writer protocol: real toolchain remove + re-read verify",
    ),
    ("cli/src/commands/core/shared.rs", "update"): (
        {"adapter-mutation"}, "legacy delegated update under gateway lock",
    ),
    ("cli/src/commands/core/install/game.rs", "install"): (
        {"adapter-mutation"}, "delegated bevy lane (cargo), documented",
    ),
    ("cli/src/commands/core/install/clo.rs", "install"): (
        {"adapter-mutation"}, "delegated cloud lane, documented",
    ),
    ("cli/src/commands/core/install/iot.rs", "install"): (
        {"adapter-mutation"}, "delegated iot lane, documented",
    ),
    # --- lock-only maintenance (writer guard held, no manifest mutation) ---
    ("cli/src/commands/dedupe.rs", "run"): (
        {"lock-write"}, "lock merge under writer guard + build-verified rollback",
    ),
    ("cli/src/commands/import.rs", "run"): (
        {"lock-write"}, "lock import under writer guard + roundtrip verify",
    ),
    # --- engine-internal lock writers (the engine, not a bypass) ---
    ("adapters/app/src/install/mod.rs", "run_install"): (
        {"lock-write"}, "engine canonical-lock writer",
    ),
    ("adapters/lib/src/install/mod.rs", "run_install"): (
        {"lock-write"}, "engine canonical-lock writer",
    ),
    ("adapters/web/src/audit.rs", "run_audit_fix"): (
        {"lock-write"}, "engine implementation; CLI entry is gateway+journaled",
    ),
    ("adapters/web/src/install/mod.rs", "run_install"): (
        {"lock-write"}, "engine lock writer under CLI gateway",
    ),
    ("core/crates/mgc-lockfile/src/writer.rs", "sign_and_write_lockfile"): (
        {"lock-write"}, "lockfile primitive (called under callers' guards)",
    ),
    # --- engine-internal manifest writers (the engine, not a bypass) ---
    ("adapters/app/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the per-language writer",
    ),
    ("adapters/game/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the cargo writer",
    ),
    ("adapters/iot/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the cargo writer",
    ),
    ("adapters/lib/src/manifest.rs", "write_cargo_manifest"): (
        {"manifest-write"}, "engine manifest writer",
    ),
    ("core/crates/mgc-adapter-base/src/cargo_manifest.rs", "write_manifest"): (
        {"manifest-write", "fs-write"}, "engine cargo writer (atomic inside)",
    ),
    # --- project creation (no live state to corrupt) ---
    ("adapters/game/src/scaffold/bevy.rs", "scaffold"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/game/src/scaffold/godot.rs", "scaffold"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/iot/src/scaffold/mod.rs", "scaffold_esp32"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/iot/src/scaffold/mod.rs", "scaffold_platformio"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/iot/src/scaffold/mod.rs", "scaffold_zephyr"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/cloud/src/scaffold/mod.rs", "scaffold_cloudflare"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/cloud/src/scaffold/mod.rs", "scaffold_cdk"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
    ("adapters/cloud/src/scaffold/mod.rs", "scaffold_pulumi"): (
        {"manifest-file-write"}, "creation-only writer",
    ),
}

# Sink inventory: (category, regex, gating?). GATING sinks fail CI
# when unallowlisted (the bypass class: manifest/lock writers and
# adapter mutations). fs-write is INVENTORY-ONLY: it is the cache/CAS/
# staging/download/telemetry substrate (~300 legitimate sites) — failing
# on all of it would train everyone to ignore the gate. Direct writes
# to *manifest files* ARE gating via manifest-file-write (filename
# literal on the same line).
MANIFEST_FILENAMES = (
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "pubspec.yaml",
    "Package.swift",
    "Podfile",
    "platformio.ini",
    "west.yml",
    "mgc.lock",
    "Pulumi.yaml",
    "wrangler.toml",
    "project.godot",
    "libs.versions.toml",
)

PATTERNS = [
    # (sink-category, regex, gating)
    ("manifest-write", re.compile(r"\.write_manifest\s*\("), True),
    ("manifest-write", re.compile(r"::write_manifest\s*\("), True),
    ("lock-write", re.compile(r"\batomic_write_locked\s*\("), True),
    ("lock-write", re.compile(r"\bwrite_web_lockfile\w*\s*\("), True),
    ("lock-write", re.compile(r"\bwrite_lockfile\s*\("), True),
    ("lock-write", re.compile(r"\bwrite_canonical_lock\s*\("), True),
    ("adapter-mutation", re.compile(r"\badapter\.add\s*\("), True),
    ("adapter-mutation", re.compile(r"\badapter\.remove\s*\("), True),
    ("adapter-mutation", re.compile(r"\badapter\.update\s*\("), True),
    ("fs-write", re.compile(r"\bstd::fs::write\s*\("), False),
    ("fs-write", re.compile(r"\btokio::fs::write\s*\("), False),
    ("fs-write", re.compile(r"\bOpenOptions\b"), False),
    ("fs-write", re.compile(r"\.write_all\s*\("), False),
    ("fs-write", re.compile(r"\bstd::fs::rename\s*\("), False),
    ("fs-write", re.compile(r"\bremove_file\s*\("), False),
    ("fs-write", re.compile(r"\bremove_dir_all\s*\("), False),
]

# The definitions themselves are not calls.
DEF_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+"
    r"(?:atomic_write_locked|write_manifest|write_web_lockfile\w*|write_lockfile|write_canonical_lock|atomic_write\w*)\b"
)

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


def scan_tree(root, scan_dirs):
    """Yield (rel, idx, line, gating-sinks, qualified) findings plus an
    fs-write inventory count. Escalation rule: an fs sink touching a
    manifest/lock filename on the SAME line is a direct manifest write
    (gating `manifest-file-write`), not substrate."""
    findings = []
    inventory_fs = 0
    for scan in scan_dirs:
        for dirpath, _dirnames, filenames in os.walk(os.path.join(root, scan)):
            for filename in filenames:
                if not filename.endswith(EXT):
                    continue
                full = os.path.join(dirpath, filename)
                rel = os.path.relpath(full, root)
                if is_test_path(rel, filename):
                    continue
                with open(full, encoding="utf-8", errors="replace") as handle:
                    lines = handle.readlines()
                for idx, line in enumerate(lines, start=1):
                    stripped = line.strip()
                    if stripped.startswith("//"):
                        continue
                    hits = [(sink, gating) for sink, pattern, gating in PATTERNS
                            if pattern.search(line)]
                    if not hits:
                        continue
                    if DEF_RE.match(line):
                        continue
                    gating = sorted({sink for sink, g in hits if g})
                    if any(name in line for name in MANIFEST_FILENAMES):
                        if "manifest-file-write" not in gating:
                            gating.append("manifest-file-write")
                        gating = sorted(set(gating))
                    if not gating:
                        inventory_fs += 1
                        continue
                    qualified = enclosing_fn(rel, idx, lines)
                    findings.append((rel, idx, line.strip(), gating, qualified))
    return findings, inventory_fs


def check_findings(findings):
    """Return violation strings with file/symbol/sink/reason."""
    violations = []
    for rel, idx, line, gating, qualified in findings:
        key = (rel, qualified)
        if key not in ALLOWLIST:
            violations.append(
                f"{rel}:{idx}: [{','.join(gating)}] fn {qualified}: "
                f"unlisted mutation caller — {line}"
            )
            continue
        # Pinned sinks: an allowlisted (file, fn) gains a NEW sink
        # category only by explicit justification.
        # (Allowlist pin từng loại sink — thêm loại mới là fail.)
        allowed_sinks, reason = ALLOWLIST[key]
        for sink in gating:
            if sink not in allowed_sinks:
                violations.append(
                    f"{rel}:{idx}: NEW sink '{sink}' in allowlisted fn {qualified} "
                    f"(allowed: {sorted(allowed_sinks)}; reason: {reason}): {line}"
                )
    return violations


# Negative-control corpus: every bypass FORM must be flagged. Each
# fixture is a minimal Rust file exercising one sink class the way a
# real bypass would (direct write, free-function writer, adapter
# mutation). The self-test asserts the gate flags ALL of them — a
# pattern change that blinds the gate fails here first.
# (Corpus kiểm âm: mọi dạng bypass phải bị bắt.)
NEGATIVE_CORPUS = {
    "direct_write_manifest.rs": ("manifest-write", "adapter.write_manifest(root, &m).await?;"),
    "free_fn_writer.rs": ("manifest-write", "crate::manifest::write_manifest(root, &m).await?;"),
    "fs_write_manifest.rs": ("manifest-file-write", 'std::fs::write(root.join("package.json"), data)?;'),
    "adapter_add_bypass.rs": ("adapter-mutation", "adapter.add(root, &name, None, opts).await?;"),
    "adapter_remove_bypass.rs": ("adapter-mutation", "adapter.remove(root, &name).await?;"),
    "adapter_update_bypass.rs": ("adapter-mutation", "adapter.update(root, None).await?;"),
    "lock_rewrite.rs": ("lock-write", "mgc_lockfile::atomic::atomic_write_locked(&guard, &p, &b, d)?;"),
}


def self_test():
    """Scan the in-memory corpus; every fixture must be flagged with
    its expected sink (gating or escalated). Returns exit code."""
    failures = []
    for name, (expected_sink, snippet) in NEGATIVE_CORPUS.items():
        lines = [
            "fn evil_bypass(adapter: &dyn Pkg, root: &std::path::Path) -> R {\n",
            f"    {snippet}\n",
            "}\n",
        ]
        rel = f"corpus/{name}"
        qualified = enclosing_fn(rel, 2, lines)
        gating = sorted({
            sink for sink, pattern, g in PATTERNS if g and pattern.search(lines[1])
        })
        if any(filename in lines[1] for filename in MANIFEST_FILENAMES):
            gating = sorted(set(gating + ["manifest-file-write"]))
        if expected_sink not in gating:
            failures.append(f"{name}: expected sink '{expected_sink}' not detected (hits={gating})")
        elif qualified != "evil_bypass":
            failures.append(f"{name}: enclosing fn misdetected as {qualified}")
    if failures:
        print("NEGATIVE-CONTROL SELF-TEST FAILURES (gate is blind):")
        for failure in failures:
            print("  " + failure)
        return 1
    print(f"OK: self-test flags all {len(NEGATIVE_CORPUS)} bypass forms")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    findings, inventory_fs = scan_tree(ROOT, SCAN_DIRS)
    violations = check_findings(findings)
    if violations:
        print("MUTATION-CALL MANIFEST VIOLATIONS (add justification to")
        print("scripts/audit_mutation_calls.py ALLOWLIST or route via gateway):")
        for violation in violations:
            print("  " + violation)
        return 1
    print(f"OK: mutation calls confined to {len(ALLOWLIST)} allowlisted sites "
          f"({inventory_fs} fs-substrate sites inventoried, non-gating)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
