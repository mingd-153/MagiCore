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
        or ("/bin/" in "/" + rel + "/" and basename.startswith("bench_"))
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
        {"lock-write", "manifest-file-write"},
        "lock+sig import under writer guard + transactional rollback",
    ),
    # --- staged template publish (locked, convergent, GC-able litter) ---
    ("cli/src/commands/core/shared.rs", "game_optimizer_template_locked"): (
        {"manifest-file-write"}, "staging publish under gateway guard",
    ),
    # --- engine-internal lock writers (the engine, not a bypass) ---
    ("adapters/app/src/install/mod.rs", "run_install"): (
        {"lock-write"}, "engine canonical-lock writer",
    ),
    ("adapters/app/src/install/mod.rs", "write_canonical_lock"): (
        {"lock-write", "manifest-file-write"}, "engine lock writer",
    ),
    ("adapters/app/src/install/mod.rs", "atomic_write_canonical_lock"): (
        {"manifest-file-write"},
        "private atomic primitive called only by write_canonical_lock under install gateway",
    ),
    ("adapters/web/src/lockfile.rs", "atomic_write_web_lockfile"): (
        {"manifest-file-write"},
        "private atomic primitive called only by write_web_lockfile_with_state under mutation gateway",
    ),
    ("adapters/lib/src/install/mod.rs", "run_install"): (
        {"lock-write"}, "engine canonical-lock writer",
    ),
    ("adapters/lib/src/install/mod.rs", "write_canonical_lock"): (
        {"lock-write", "manifest-file-write"}, "engine lock writer",
    ),
    ("adapters/web/src/install/mod.rs", "run_install"): (
        {"lock-write", "manifest-file-write"},
        "engine lock writer + staging/scratch tree management under CLI gateway",
    ),
    ("adapters/web/src/install/mod.rs", "restore_prior_lock_result"): (
        {"manifest-file-write"}, "lock bytes restore helper (verified by callers)",
    ),
    ("adapters/web/src/lockfile.rs", "write_web_lockfile_with_state"): (
        {"manifest-write", "manifest-file-write"}, "engine lock writer",
    ),
    ("adapters/web/src/cache_prune.rs", "prune_old_package_dirs_under"): (
        {"manifest-file-write"}, "stale package-dir pruning (age-gated)",
    ),
    ("adapters/web/src/install/extract.rs", "ensure_extracted_package_root_with_marker"): (
        {"manifest-file-write"}, "extract staging management",
    ),
    # --- delegation wrappers (forward to the owned engine; no direct IO) ---
    ("adapters/cloud/src/adapter.rs", "DependencyResolver::add"): (
        {"adapter-mutation"}, "forwards to the embedded web engine",
    ),
    ("adapters/cloud/src/adapter.rs", "DependencyResolver::remove"): (
        {"adapter-mutation"}, "forwards to the embedded web engine",
    ),
    ("adapters/cloud/src/adapter.rs", "DependencyResolver::update"): (
        {"adapter-mutation"}, "forwards to the embedded web engine",
    ),
    ("adapters/lib/src/adapter.rs", "DependencyResolver::add"): (
        {"adapter-mutation", "manifest-write"},
        "native writer implementation; CLI entry is shared::add under the project mutation gateway",
    ),
    ("adapters/lib/src/adapter.rs", "DependencyResolver::remove"): (
        {"adapter-mutation", "manifest-write"},
        "native writer implementation; CLI entry is shared::remove under the project mutation gateway",
    ),
    ("adapters/lib/src/adapter.rs", "DependencyResolver::update"): (
        {"adapter-mutation", "manifest-write"},
        "native writer implementation; CLI entry is shared::update under the project mutation gateway",
    ),
    ("adapters/lib/src/install/mod.rs", "atomic_write_canonical_lock"): (
        {"manifest-file-write"}, "atomic mgc.lock publication called from native install under CLI gateway",
    ),
    ("core/crates/mgc-types/src/adapter.rs", "prepare_add"): (
        {"adapter-mutation"}, "trait default dry-run probe via own add",
    ),
    ("adapters/web/src/audit.rs", "run_audit_fix"): (
        {"lock-write"}, "engine implementation; CLI entry is gateway+journaled",
    ),

    ("core/crates/mgc-lockfile/src/writer.rs", "sign_and_write_lockfile"): (
        {"lock-write"}, "lockfile primitive (called under callers' guards)",
    ),
    # --- engine-internal manifest writers (the engine, not a bypass) ---
    ("adapters/app/src/adapter.rs", "AppAdapter::remove_swift_native"): (
        {"manifest-file-write"}, "engine writer under CLI gateway (app remove lane)",
    ),
    ("adapters/app/src/adapter.rs", "AppAdapter::remove_kotlin_native"): (
        {"manifest-file-write"}, "engine writer under CLI gateway (app remove lane)",
    ),
    ("adapters/app/src/adapter.rs", "AppAdapter::update_swift_native"): (
        {"manifest-file-write"}, "engine writer under CLI gateway (app update lane)",
    ),
    ("adapters/app/src/manifest/flutter.rs", "write_pubspec"): (
        {"manifest-file-write"}, "engine manifest writer",
    ),
    ("adapters/app/src/manifest/gradle.rs", "bump_catalog_pin"): (
        {"manifest-file-write"}, "engine manifest writer",
    ),
    ("adapters/app/src/install/mod.rs", "write_canonical_lock"): (
        {"lock-write", "manifest-file-write"}, "engine lock writer",
    ),
    ("adapters/lib/src/install/mod.rs", "run_install"): (
        {"lock-write"}, "engine canonical-lock writer",
    ),
    ("adapters/lib/src/manifest.rs", "write_pom_manifest"): (
        {"manifest-file-write"}, "engine manifest writer",
    ),
    ("adapters/lib/src/manifest.rs", "write_csproj_manifest"): (
        {"manifest-file-write"}, "engine manifest writer",
    ),
    ("adapters/lib/src/manifest.rs", "write_go_mod_manifest"): (
        {"manifest-file-write"}, "engine manifest writer",
    ),
    ("adapters/lib/src/manifest.rs", "write_pyproject_manifest"): (
        {"manifest-file-write"}, "engine manifest writer",
    ),
    # --- link/staging/scratch management (not manifest content) ---
    ("cli/src/commands/core/web.rs", "link_monorepo_workspace_packages"): (
        {"manifest-file-write"}, "symlink management only, no content writes",
    ),
    # --- non-manifest outputs (reports, scaffolds, generated files) ---
    ("cli/src/commands/dlx.rs", "run"): (
        {"manifest-file-write"}, "writes dlx output, not project manifests",
    ),
    ("cli/src/commands/sbom.rs", "run"): (
        {"manifest-file-write"}, "writes SBOM report output",
    ),
    ("cli/src/commands/bench.rs", "handle"): (
        {"manifest-file-write"}, "removes temp lock for fresh-resolve measurement",
    ),
    ("cli/src/commands/core/update/ai.rs", "update"): (
        {"manifest-file-write"}, "engine requirements.lock writer (ai lane)",
    ),
    ("cli/src/commands/core/dev/ai_docker.rs", "generate_ai_docker_files"): (
        {"manifest-file-write"}, "generates Dockerfiles, not manifests",
    ),
    ("adapters/app/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the per-language writer",
    ),
    ("adapters/app/src/manifest/react_native.rs", "write_package_json"): (
        {"manifest-write"}, "React Native package.json writer delegates to Web engine; CLI mutation entry is gateway-routed",
    ),
    ("adapters/game/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the cargo writer",
    ),
    ("adapters/iot/src/adapter.rs", "LockfileProvider::write_manifest"): (
        {"manifest-write"}, "delegating override to the cargo writer",
    ),
    ("adapters/ai/src/adapter.rs", "mgc_types::remove"): (
        {"adapter-mutation"}, "forwards native Python removal; CLI entry is shared::remove under mutation gateway",
    ),
    ("adapters/ai/src/adapter.rs", "mgc_types::update"): (
        {"adapter-mutation"}, "forwards native Python update; CLI entry is shared::update under mutation gateway",
    ),
    ("adapters/ai/src/adapter.rs", "mgc_types::write_manifest"): (
        {"manifest-write"}, "forwards native Python manifest write; CLI mutation entry is gateway-routed",
    ),
    ("adapters/lib/src/manifest.rs", "write_cargo_manifest"): (
        {"manifest-write"}, "engine manifest writer",
    ),
    ("core/crates/mgc-adapter-base/src/cargo_manifest.rs", "write_manifest"): (
        {"manifest-write", "fs-write", "manifest-file-write"}, "engine cargo writer (atomic inside)",
    ),
    ("core/crates/mgc-resolver/src/protocols/swift.rs", "SwiftRegistryProtocol::resolve_git"): (
        {"manifest-file-write"}, "git workdir scratch cleanup",
    ),
    ("core/crates/mgc-lockfile/src/atomic.rs", "cleanup_stale_temps"): (
        {"manifest-file-write"}, "stale temp sweeper (age+grace gated)",
    ),
    ("cli/src/commands/publish.rs", "publish_project"): (
        {"manifest-file-write"}, "release version commit under writer guard",
    ),
    ("cli/src/commands/core/shared.rs", "restore_mutation_snapshot"): (
        {"manifest-write", "lock-write", "manifest-file-write"},
        "verified restore path (incl. lock-absence restore)",
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

# No duplicate keys: a shadowed entry silently loses its sinks (caught
# the hard way — a narrower duplicate overrode fuller coverage).
# (Không key trùng — entry sau đè entry trước trong dict.)
_DUPLICATE_CHECK: dict = {}
for _key in ALLOWLIST:
    assert _key not in _DUPLICATE_CHECK, f"duplicate allowlist key: {_key}"
    _DUPLICATE_CHECK[_key] = True
del _DUPLICATE_CHECK

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
    # Any `.<add|remove|update>(root-ish, ...)` — catches engine/tool/
    # provider variables whatever they are named (the web-lane bypass
    # spelled it `adapter`, the next one may not).
    # (Mọi .add/remove/update với root-ish — bất kể tên biến.)
    ("adapter-mutation", re.compile(r"\.(?:add|remove|update)\s*\(\s*(?:root|project_root|self|&root|ctx|project)\b"), True),
    # Aliased fs writes (bare `fs::` after `use std::fs`, `File::create`
    # for truncate-create). Same sink class as std::fs::write.
    # (Alias fs — cùng loại sink.)
    ("fs-write", re.compile(r"\bfs::write\s*\("), False),
    ("fs-write", re.compile(r"\bFile::create\s*\("), False),
    ("fs-write", re.compile(r"\bfs::rename\s*\("), False),
    ("fs-write", re.compile(r"\bfs::remove_file\s*\("), False),
    ("fs-write", re.compile(r"\bfs::remove_dir_all\s*\("), False),
    ("fs-write", re.compile(r"\bstd::fs::write\s*\("), False),
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
                # Functions mentioning a manifest/lock filename ANYWHERE
                # in their CODE body (line comments excluded — string
                # literals KEPT, since `root.join("package.json")` is
                # one; block comments may over-trigger, acceptable).
                # (Hàm nào nhắc tên manifest trong CODE thì fs trong nó bị soi.)
                manifest_fns = set()
                for idx, line in enumerate(lines, start=1):
                    code = line.split("//")[0]
                    if any(name in code for name in MANIFEST_FILENAMES):
                        qualified = enclosing_fn(rel, idx, lines)
                        if qualified:
                            manifest_fns.add(qualified)
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
                    qualified = enclosing_fn(rel, idx, lines)
                    if any(name in line for name in MANIFEST_FILENAMES):
                        gating.append("manifest-file-write")
                    elif (qualified in manifest_fns and
                            any(s == "fs-write" for s, _g in hits)):
                        gating.append("manifest-file-write")
                    gating = sorted(set(gating))
                    if not gating:
                        inventory_fs += 1
                        continue
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
    "fs_alias_write_manifest.rs": ("manifest-file-write", "fs::write(p.join(\"Cargo.toml\"), data)?;"),
    "file_create_manifest.rs": ("manifest-file-write", "std::fs::File::create(root.join(\"pyproject.toml\"))?;"),
    "split_let_write_manifest.rs": ("manifest-file-write", "fs::write(p, data)?;"),
    "adapter_add_bypass.rs": ("adapter-mutation", "adapter.add(root, &name, None, opts).await?;"),
    "engine_named_add_bypass.rs": ("adapter-mutation", "engine.add(root, &name, None, opts).await?;"),
    "adapter_remove_bypass.rs": ("adapter-mutation", "adapter.remove(root, &name).await?;"),
    "adapter_update_bypass.rs": ("adapter-mutation", "adapter.update(root, None).await?;"),
    "lock_rewrite.rs": ("lock-write", "mgc_lockfile::atomic::atomic_write_locked(&guard, &p, &b, d)?;"),
}


def self_test():
    """Write the corpus to a temp tree and run the REAL scan_tree +
    check_findings pipeline: every fixture must come back as a gating
    violation (unlisted by design). This proves the production path —
    not a reimplementation — flags every bypass form. Returns exit code."""
    import tempfile
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, (expected_sink, snippet) in NEGATIVE_CORPUS.items():
            if name == "split_let_write_manifest.rs":
                body = (
                    "fn evil_bypass(root: &std::path::Path) -> R {\n"
                    '    let p = root.join("package.json");\n'
                    f"    {snippet}\n"
                    "}\n"
                )
            else:
                body = (
                    "fn evil_bypass(adapter: &dyn Pkg, root: &std::path::Path) -> R {\n"
                    f"    {snippet}\n"
                    "}\n"
                )
            with open(os.path.join(tmp, name), "w", encoding="utf-8") as handle:
                handle.write(body)
        findings, _inventory = scan_tree(tmp, ["."])
        violations = check_findings(findings)
        by_file = {}
        for violation in violations:
            # scan uses tmp-relative rels ("name.rs"); match by basename.
            # (So khớp theo basename.)
            for name in NEGATIVE_CORPUS:
                if violation.startswith(name + ":"):
                    by_file.setdefault(name, []).append(violation)
        for name, (expected_sink, _snippet) in NEGATIVE_CORPUS.items():
            hits = by_file.get(name, [])
            if not hits:
                failures.append(f"{name}: not flagged at all")
            elif not any(expected_sink in hit for hit in hits):
                failures.append(
                    f"{name}: expected sink '{expected_sink}' missing: {hits}"
                )
    if failures:
        print("NEGATIVE-CONTROL SELF-TEST FAILURES (gate is blind):")
        for failure in failures:
            print("  " + failure)
        return 1
    print(f"OK: self-test flags all {len(NEGATIVE_CORPUS)} bypass forms via the real pipeline")
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
