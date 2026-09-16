#!/usr/bin/env python3
"""P0-A dependency-delegation audit gate (2026-09-16).

V1.2 audit contract: MagiCore must become a NATIVE dependency engine; until
the native engines land (Phase 2/3), every place where a dependency-lifecycle
operation still shells out to an external package manager must be
MEASURABLE and DECLARED. This gate is the measurement — it scans the
dependency lifecycle code for spawns of the forbidden PM toolchains and
classifies each one:

  allowed               the operation is NOT a dependency lifecycle op
                        (mgc build/test/run/dev/flash/doctor lanes — the
                        user's file/operation whitelist), or the file lives
                        under a test/ tree.
  delegated-documented  the spawn sits in a function (or its doc block)
                        carrying the `DELEGATED:` marker — an explicitly
                        declared, honest delegation (Việc 1 / P0-B marker
                        convention: "mgc does not own this lifecycle").
  violation             a dependency-lifecycle spawn that is NEITHER
                        whitelisted NOR documented — the gate FAILS.

Output: gitignored JSON ledger at docs/specs/dependencyDelegationAudit.json
(every finding: file, line, tool, op_class, status) so Phase 2/3 native
engine work can be driven from an exact list instead of prose. Exit code 1
when at least one UNdocumented violation exists, 0 otherwise.

Heuristics (documented, deliberately conservative): Rust is scanned
line/context based — function bodies are delimited by a naive brace walk,
and a `DELEGATED:` marker anywhere in the enclosing function (or its
contiguous comment block above) covers every spawn inside it. Spawn shapes
recognised: exec_tool(), run()/run_inherited()/run_capture(), mgc_run(),
Command::new(), which()/which::which(), `(tool, args)` tables, `tool: "x"`
fields, `== "x"` dispatch, and a bare tool literal on its own line.

Cổng audit uỷ quyền dependency P0-A (2026-09-16): mọi chỗ lifecycle
dependency còn gọi toolchain ngoài phải ĐO ĐƯỢC và ĐÃ KHAI. Script quét code
lifecycle dependency tìm spawn của toolchain PM bị cấm và phân loại:
`allowed` (không phải operation dependency — lane build/test/run/dev/flash/
doctor, hoặc file trong cây test), `delegated-documented` (hàm/chú thích
mang marker `DELEGATED:` — uỷ quyền khai báo trung thực), `violation`
(spawn dependency KHÔNG whitelist, KHÔNG khai → gate FAIL). JSON ledger
gitignored ở docs/specs/dependencyDelegationAudit.json. Exit 1 khi còn
violation chưa khai, 0 khi sạch.
"""

import datetime
import json
import os
import re
import subprocess
import sys

# Roots that hold the dependency lifecycle: every core adapter plus the CLI
# install/add/remove/update lanes.
# (Gốc chứa lifecycle dependency: mọi adapter core + lane CLI
# install/add/remove/update.)
SCAN_ROOTS = [
    "adapters",
    "cli/src/commands/core/install",
    "cli/src/commands/core/add",
    "cli/src/commands/core/remove",
    "cli/src/commands/core/update",
]

# The forbidden PM toolchains (V1.2 audit list) — spawning any of these in a
# dependency operation is delegation that must be declared.
# (Toolchain PM bị cấm — spawn trong operation dependency là uỷ quyền phải
# khai báo.)
TOOLS = [
    "cargo", "uv", "pip", "pip3", "go", "gradle", "mvn", "dotnet", "flutter",
    "dart", "swift", "pod", "npm", "pnpm", "yarn", "bun", "deno", "pio",
    "west", "terraform",
]

# Non-dependency mgc lanes: tool spawns here are the lane's own business
# (build/test/run/dev/flash/doctor), never dependency resolution. Path
# segments also accept the plural/bench spellings (tests/, benches/) —
# test code and measurement harnesses are not the product's dependency
# lifecycle.
# (Lane không phải dependency: spawn ở đây là việc của chính lane đó.
# Segment đường dẫn chấp nhận cả số nhiều/bench (tests/, benches/) — code
# test và harness đo đạc không phải lifecycle dependency của product.)
ALLOWED_OPS = ("build", "test", "run", "dev", "flash", "doctor")

# Extra PATH-ONLY whitelist segments (never fn-name matches): test trees
# and bench harnesses.
# (Segment CHỈ-whitelist-đường-dẫn (không dùng cho tên hàm): cây test và
# harness bench.)
ALLOWED_PATH_SEGMENTS = ("test", "bench")

# Dependency lifecycle operations — when one of these words names the
# function or a path segment, the file is in dependency scope even if the
# function name also carries a whitelist word (e.g. `run_install`).
# (Operation lifecycle dependency — khi một từ này nằm trong tên hàm hay
# segment đường dẫn, file thuộc phạm vi dependency dù tên hàm có cả từ
# whitelist, vd `run_install`.)
DEPENDENCY_OPS = (
    "install", "add", "remove", "update", "resolve", "fetch", "lock", "cache",
)

# The declaration marker (Việc 1 / P0-B convention).
# (Marker khai báo uỷ quyền (quy ước Việc 1 / P0-B).)
MARKER = "DELEGATED:"

OUTPUT_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    os.pardir,
    "docs",
    "specs",
    "dependencyDelegationAudit.json",
)

_TOOL_ALT = "|".join(re.escape(tool) for tool in TOOLS)

# Spawn-shaped contexts. Applied to the whole file text (so a tool literal
# on its own line inside a multi-line `run(\n "cargo",` call still matches),
# then mapped back to a line number.
# (Ngữ cảnh hình-spawn. Áp trên cả text file (để literal tool nằm dòng riêng
# trong call nhiều dòng vẫn khớp), rồi map về số dòng.)
SPAWN_PATTERNS = [
    re.compile(rf'\bexec_tool\s*\([^()]*?"({_TOOL_ALT})"'),
    re.compile(rf'(?<![\w_])(?:run_inherited|run_capture|run)\s*\(\s*"({_TOOL_ALT})"'),
    re.compile(rf'\bmgc_run\s*\(\s*"({_TOOL_ALT})"'),
    re.compile(rf'\bCommand::new\s*\(\s*"({_TOOL_ALT})"'),
    re.compile(rf'\bwhich(?:::which)?\s*\(\s*"({_TOOL_ALT})"'),
    # (tool, args) tables — `("cdk", vec!["deploy"])`, `("uv", vec![…])`.
    re.compile(rf'\(\s*"({_TOOL_ALT})"\s*,\s*(?:&?\[|vec!)'),
    # Spawn descriptor fields — `tool: "flutter".to_string()`.
    re.compile(rf'\btool\s*:\s*"({_TOOL_ALT})"'),
    # Dispatch comparisons against a TOOL VARIABLE — `tool == "uv"`,
    # `kind != "terraform"`. A bare `name == "flutter"` (dependency-name
    # iteration inside a manifest parser) is NOT a spawn and must not
    # match, so the left-hand side must name the tool being chosen.
    # (So sánh điều phối với BIẾN TOOL — `tool == "uv"`, `kind !=
    # "terraform"`. `name == "flutter"` (duyệt tên dependency trong parser
    # manifest) KHÔNG phải spawn nên không được khớp — vế trái phải là tên
    # biến đang chọn tool.)
    re.compile(rf'\b(?:tool|kind|cmd|bin)\s*(?:==|!=)\s*"({_TOOL_ALT})"'),
    re.compile(rf'"({_TOOL_ALT})"\s*==\s*(?:tool|kind|cmd|bin)\b'),
    # A bare tool literal alone on its line (variable assignment / call arg).
    re.compile(rf'^\s*"({_TOOL_ALT})",?\s*$', re.MULTILINE),
]

# Contexts that only LOOK like spawns: metadata labels and cache-directory
# naming never execute the tool.
# (Ngữ cảnh trông giống spawn nhưng không chạy tool: nhãn metadata và đặt
# tên thư mục cache.)
EXCLUDE_PATTERNS = [
    re.compile(r'\becosystem\s*[:=]'),
    re.compile(r'\bshared_root\s*\('),
    re.compile(r'^\s*//'),
    re.compile(r'^\s*#'),
]

FN_RE = re.compile(
    r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)'
)


def _iter_rust_files() -> list:
    """Every .rs file under the scan roots (absolute, sorted).
    (Mọi file .rs dưới scan roots (tuyệt đối, đã sắp xếp).)"""
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    files = []
    for root in SCAN_ROOTS:
        base = os.path.join(repo_root, root)
        for dirpath, _dirnames, filenames in os.walk(base):
            for name in sorted(filenames):
                if name.endswith(".rs"):
                    files.append(os.path.join(dirpath, name))
    return sorted(files)


def _functions(lines: list) -> list:
    """Naive function spans: (name, start_idx, end_idx) via a brace walk.
    Heuristic by design — a Rust file whose string literals carry
    unbalanced braces can only ever make a span too long, never invent a
    function out of thin air.
    (Span hàm thô: (tên, dòng_đầu, dòng_cuối) qua đếm ngoặc. Cố ý heuristic
    — literal có ngoặc lệch chỉ làm span dài quá, không bao giờ sinh hàm
    từ hư không.)"""
    spans = []
    for idx, line in enumerate(lines):
        match = FN_RE.match(line)
        if not match:
            continue
        depth = 0
        started = False
        end = idx
        for cursor in range(idx, len(lines)):
            depth += lines[cursor].count("{") - lines[cursor].count("}")
            if "{" in lines[cursor]:
                started = True
            if started and depth <= 0:
                end = cursor
                break
        spans.append((match.group(1), idx, end))
    return spans


def _enclosing_function(spans: list, line_idx: int):
    """The INNERMOST function span containing the line, or None.
    (Span hàm TRONG CÙNG chứa dòng đó, hoặc None.)"""
    best = None
    for name, start, end in spans:
        if start <= line_idx <= end:
            if best is None or (end - start) < (best[2] - best[1]):
                best = (name, start, end)
    return best


def _doc_block(lines: list, start_idx: int) -> str:
    """Contiguous comment lines directly above a function start, skipping
    attribute lines (`#[allow(...)]`) that sit between the docs and the
    fn — they are part of the same declaration.
    (Các dòng chú thích liền trên điểm bắt đầu hàm, bỏ qua dòng attribute
    (`#[allow(...)]`) nằm giữa chú thích và fn — attribute thuộc cùng một
    khai báo.)"""
    collected = []
    cursor = start_idx - 1
    while cursor >= 0:
        stripped = lines[cursor].strip()
        if stripped.startswith("//"):
            collected.append(lines[cursor])
        elif stripped.startswith("#[") or stripped.startswith("#!"):
            pass  # attribute between doc and fn — keep walking (bỏ qua)
        else:
            break
        cursor -= 1
    return "\n".join(reversed(collected))


def _classify(rel_path: str, fn_name: str, fn_lines: list, doc_text: str):
    """(op_class, status) for one finding.
    ((op_class, status) cho một finding.)"""
    parts = rel_path.replace(os.sep, "/").split("/")
    path_parts = [p.lower() for p in parts[:-1]]
    fn_lower = fn_name.lower()
    fn_parts = [p for p in fn_lower.split("_") if p]
    body = "\n".join(fn_lines)
    marked = MARKER in body or MARKER in doc_text

    # 1) Path-level whitelist wins first: a file under a dev/test/flash
    #    tree is that lane's code, even when a parent segment says
    #    install/add (`cli/.../add/test/ai.rs`). Plural and bench
    #    spellings (tests/, benches/) count — tests and measurement
    #    harnesses are not the dependency lifecycle.
    #    (Whitelist mức đường dẫn thắng trước: file dưới cây dev/test/
    #    flash là code của lane đó, dù segment cha là install/add. Số
    #    nhiều và bench (tests/, benches/) cũng tính — test và harness đo
    #    đạc không phải lifecycle dependency.)
    for segment in path_parts:
        matched = next(
            (op for op in ALLOWED_OPS + ALLOWED_PATH_SEGMENTS
             if segment == op or segment.startswith(op)),
            None,
        )
        if matched is not None:
            return matched, "allowed"

    # 2) Dependency scope wins over fn-name whitelist words: `run_install`
    #    must never be whitelisted by the `run` keyword in its name.
    #    (Phạm vi dependency thắng các từ whitelist trong tên hàm:
    #    `run_install` không được whitelist nhờ từ `run` trong tên.)
    for op in DEPENDENCY_OPS:
        if op in fn_parts or op in fn_lower or op in path_parts:
            return op, ("delegated-documented" if marked else "violation")

    # 3) Whitelisted operation by function name (run_test_step, flash_fw,
    #    doctor_store, build_release, dev_server…).
    #    (Operation whitelist theo tên hàm.)
    for op in ALLOWED_OPS:
        if op in fn_parts or fn_lower.startswith(op):
            # Declared delegation stays visible even on a whitelisted op —
            # more information, never less.
            # (Uỷ quyền đã khai vẫn hiện kể cả trên op whitelist — nhiều
            # thông tin hơn, không bao giờ ít hơn.)
            return op, ("delegated-documented" if marked else "allowed")

    # 4) Unclassified spawn: the honest default is a violation unless the
    #    DELEGATED: marker declares it.
    #    (Spawn chưa phân loại: mặc định trung thực là violation trừ khi
    #    marker `DELEGATED:` khai rõ.)
    return "unclassified", ("delegated-documented" if marked else "violation")


def _line_excluded(line: str) -> bool:
    """True when the physical line is a non-spawn context (comment, label).
    (True khi dòng vật lý là ngữ cảnh không-spawn (chú thích, nhãn).)"""
    return any(pattern.search(line) for pattern in EXCLUDE_PATTERNS)


def scan_file(rel_path: str, abs_path: str) -> list:
    """All findings in one file.
    (Mọi finding trong một file.)"""
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as handle:
            text = handle.read()
    except OSError:
        return []
    lines = text.splitlines()
    spans = _functions(lines)
    findings = []

    for pattern in SPAWN_PATTERNS:
        for match in pattern.finditer(text):
            tool = match.group(1)
            line_no = text.count("\n", 0, match.start()) + 1
            line = lines[line_no - 1] if line_no <= len(lines) else ""
            if _line_excluded(line):
                continue
            span = _enclosing_function(spans, line_no - 1)
            if span is None:
                fn_name, fn_lines, doc_text = "<top-level>", [line], ""
            else:
                name, start, end = span
                fn_name = name
                fn_lines = lines[start:end + 1]
                doc_text = _doc_block(lines, start)
            op_class, status = _classify(rel_path, fn_name, fn_lines, doc_text)
            findings.append({
                "file": rel_path,
                "line": line_no,
                "tool": tool,
                "op_class": op_class,
                "function": fn_name,
                "status": status,
            })

    # Deduplicate: one finding per (file, line, tool) — a line matching two
    # shapes is still one spawn.
    # (Khử trùng: một finding mỗi (file, dòng, tool) — dòng khớp hai dạng
    # vẫn chỉ là một spawn.)
    unique = {}
    for finding in findings:
        key = (finding["file"], finding["line"], finding["tool"])
        unique.setdefault(key, finding)
    return sorted(unique.values(), key=lambda f: (f["file"], f["line"], f["tool"]))


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    findings = []
    for abs_path in _iter_rust_files():
        rel_path = os.path.relpath(abs_path, repo_root)
        findings.extend(scan_file(rel_path, abs_path))

    counts = {"allowed": 0, "delegated-documented": 0, "violation": 0}
    for finding in findings:
        counts[finding["status"]] = counts.get(finding["status"], 0) + 1

    try:
        commit = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root, capture_output=True, text=True,
        ).stdout.strip()
    except OSError:
        commit = ""

    report = {
        "schema": "dependency-delegation-audit/1",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ),
        "commit": commit,
        "scan_roots": SCAN_ROOTS,
        "tools": TOOLS,
        "allowed_ops": list(ALLOWED_OPS),
        "declaration_marker": MARKER,
        "summary": {
            "total": len(findings),
            "allowed": counts["allowed"],
            "delegated_documented": counts["delegated-documented"],
            "violation": counts["violation"],
        },
        "findings": findings,
    }

    out_path = os.path.abspath(os.path.join(repo_root, OUTPUT_PATH))
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2)
        handle.write("\n")

    print(
        f"dependency-delegation audit: {len(findings)} finding(s) — "
        f"allowed={counts['allowed']} "
        f"delegated-documented={counts['delegated-documented']} "
        f"violation={counts['violation']}"
    )
    for finding in findings:
        if finding["status"] == "delegated-documented":
            print(
                f"  [delegated] {finding['file']}:{finding['line']} "
                f"{finding['tool']} ({finding['function']})"
            )
    violated = [f for f in findings if f["status"] == "violation"]
    if violated:
        print(
            "UNDOCUMENTED DEPENDENCY DELEGATION — add a `DELEGATED:` marker "
            "or make the lane fail closed:",
            file=sys.stderr,
        )
        for finding in violated:
            print(
                f"  [violation] {finding['file']}:{finding['line']} "
                f"{finding['tool']} ({finding['function']})",
                file=sys.stderr,
            )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    print(f"ledger: {os.path.relpath(out_path, repo_root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
