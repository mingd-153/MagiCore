#!/usr/bin/env python3
"""P0-A dependency-delegation audit gate (2026-09-16).

V1.2 audit contract: MagiCore must become a NATIVE dependency engine. Every
external dependency-lifecycle tool spawn is a blocker; documentation cannot
waive it. This gate scans dependency lifecycle code and classifies findings:

  allowed               the operation is NOT a dependency lifecycle op
                        (mgc build/test/run/dev/flash/doctor lanes — the
                        user's file/operation whitelist), or the file lives
                        under a test/ tree.
  violation             every dependency-lifecycle spawn, whether or not
                        it carries a `DELEGATED:` marker — the gate FAILS.
                        A marker is evidence/ledger metadata, never a waiver.

Output: gitignored JSON ledger at docs/specs/dependencyDelegationAudit.json
(every finding: file, line, tool, op_class, status) so Phase 2/3 native
engine work can be driven from an exact list instead of prose. Exit code 1
when any dependency-lifecycle delegation exists, 0 otherwise.

Heuristics (documented, deliberately conservative): Rust is scanned
line/context based — function bodies are delimited by a naive brace walk,
and a `DELEGATED:` marker anywhere in the enclosing function (or its
contiguous comment block above) covers every spawn inside it. Spawn shapes
recognised: exec_tool(), run()/run_inherited()/run_capture(), mgc_run(),
Command::new(), which()/which::which(), `(tool, args)` tables, `tool: "x"`
fields, `== "x"` dispatch, and a bare tool literal on its own line.

Cổng ownership native: mọi spawn package-manager trong dependency operation
đều là blocker, kể cả khi có marker `DELEGATED:`. Marker chỉ ghi nhận debt,
không miễn trừ. `allowed` chỉ dành cho operation ngoài dependency scope
đã khai báo. Exit 1 khi còn spawn delegated trong dependency lifecycle.
"""

import datetime
import json
import os
import re
import subprocess
import sys

# Roots that hold the dependency lifecycle: every core adapter plus the CLI
# install/add/remove/update lanes, plus the shared audit engine (external
# scanners are delegation), the hooks dispatcher (hook programs run on
# dependency events), and the model lane (python quantize passthrough).
# (Gốc chứa lifecycle dependency: mọi adapter core + lane CLI
# install/add/remove/update, cộng engine audit dùng chung (scanner ngoài là
# uỷ quyền), bộ điều phối hooks (chương trình hook chạy trên event
# dependency), và lane model (quantize passthrough python).)
SCAN_ROOTS = [
    "adapters",
    "cli/src/commands/core/install",
    "cli/src/commands/core/add",
    "cli/src/commands/core/remove",
    "cli/src/commands/core/update",
    # Read-only dependency inventory can still invoke a package manager
    # (for example `list` implementations that probe installed state).
    "cli/src/commands/core/list",
    "cli/src/commands/core/web.rs",
    "cli/src/commands/core/shared.rs",
    # Model quantization is declared core-owned by this gate; include the
    # actual command tree so python/toolchain subprocesses cannot escape.
    "cli/src/commands/model",
    "cli/src/commands/publish.rs",
    "core/crates/mgc-audit/src",
    "core/crates/mgc-config/src",
]

# The forbidden PM toolchains (V1.2 audit list) — spawning any of these in a
# dependency operation is delegation that must be declared. python/python3
# cover model-quantize passthrough; pip-audit/cargo-audit/govulncheck cover
# audit-via-external-scanner (advisory metadata is a core-owned operation).
# (Toolchain PM bị cấm — spawn trong operation dependency là uỷ quyền phải
# khai báo. python/python3 phủ quantize passthrough; pip-audit/cargo-audit/
# govulncheck phủ audit qua scanner ngoài (metadata advisory là việc cốt lõi
# MGC phải sở hữu).)
TOOLS = [
    "cargo", "uv", "pip", "pip3", "go", "gradle", "mvn", "dotnet", "flutter",
    "dart", "swift", "pod", "npm", "pnpm", "yarn", "bun", "deno", "pio",
    "west", "terraform", "git", "python", "python3", "pip-audit", "cargo-audit",
    "govulncheck", "composer", "pub",
]

# Non-dependency mgc lanes: tool spawns here are the lane's own business
# (build/test/run/dev/flash/doctor), never dependency resolution. Path
# segments also accept the plural/bench spellings (tests/, benches/) —
# test code and measurement harnesses are not the product's dependency
# lifecycle.
# (Lane không phải dependency: spawn ở đây là việc của chính lane đó.
# Segment đường dẫn chấp nhận cả số nhiều/bench (tests/, benches/) — code
# test và harness đo đạc không phải lifecycle dependency của product.)
ALLOWED_OPS = ("build", "test", "run", "dev", "flash", "doctor", "deploy")

# Extra PATH-ONLY whitelist segments (never fn-name matches): test trees
# and bench harnesses.
# (Segment CHỈ-whitelist-đường-dẫn (không dùng cho tên hàm): cây test và
# harness bench.)
ALLOWED_PATH_SEGMENTS = ("test", "bench")

# Dependency lifecycle operations — when one of these words names the
# function or a path segment, the file is in dependency scope even if the
# function name also carries a whitelist word (e.g. `run_install`). audit /
# scan / hook / quantize are core-owned operations too: advisory metadata,
# hook dispatch on dependency events, and model quantization must not
# silently shell out to external toolchains.
# (Operation lifecycle dependency — khi một từ này nằm trong tên hàm hay
# segment đường dẫn, file thuộc phạm vi dependency dù tên hàm có cả từ
# whitelist, vd `run_install`. audit/scan/hook/quantize cũng là việc cốt
# lõi: metadata advisory, điều phối hook trên event dependency và quantize
# model không được âm thầm gọi toolchain ngoài.)
DEPENDENCY_OPS = (
    "install", "add", "remove", "update", "resolve", "fetch", "lock", "cache",
    "audit", "scan", "hook", "quantize",
)

# The declaration marker (Việc 1 / P0-B convention).
# (Marker khai báo uỷ quyền (quy ước Việc 1 / P0-B).)
MARKER = "DELEGATED:"

# Lane files that MUST pass through the C0 ownership firewall: every
# dependency-lane module (logic, not pure routers) must reference the
# gate (`dep_gate::`) or carry an explicit `GATE-EXEMPT:` reason.
# (File lane PHẢI qua tường lửa C0: mọi module lane dependency (có logic,
# không phải router thuần) phải nhắc `dep_gate::` hoặc ghi rõ lý do
# `GATE-EXEMPT:`.)
LANE_GATE_ROOTS = [
    "cli/src/commands/core/install",
    "cli/src/commands/core/add",
    "cli/src/commands/core/remove",
    "cli/src/commands/core/update",
    "cli/src/commands/core/list",
]

OUTPUT_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    os.pardir,
    "docs",
    "specs",
    "dependencyDelegationAudit.json",
)

_TOOL_ALT = "|".join(re.escape(tool) for tool in TOOLS)

# Message macros whose string arguments NEVER execute as processes
# (`format!("...deno...")` names the tool in prose, it does not spawn it).
# The weak bare-literal pattern is skipped inside these spans; every
# call-shaped pattern (exec_tool/run/Command::new/which/tool-tables)
# still matches inside them, so a real spawn can never hide behind a
# message macro.
# (Macro message mà đối số chuỗi KHÔNG BAO GIỜ chạy như process
# (`format!("...deno...")` chỉ nhắc tool trong văn bản, không spawn.
# Pattern bare-literal yếu được bỏ qua trong các span này; mọi pattern
# dạng-call vẫn khớp bên trong, nên spawn thật không thể núp sau macro
# message.)
MESSAGE_MACROS = (
    "format!", "eprintln!", "println!", "bail!", "anyhow!", "panic!",
    "assert!", "debug_assert!", "unreachable!", "todo!", "unimplemented!",
    "write!", "writeln!", "eprint!", "print!",
)

# Spawn-shaped contexts. Applied to the whole file text (so a tool literal
# on its own line inside a multi-line `run(\n "cargo",` call still matches),
# then mapped back to a line number. Each entry is (pattern,
# matches_inside_message): only the weak bare-literal pattern is silenced
# inside message-macro spans (see MESSAGE_MACROS).
# (Ngữ cảnh hình-spawn. Áp trên cả text file (để literal tool nằm dòng riêng
# trong call nhiều dòng vẫn khớp), rồi map về số dòng. Mỗi entry là
# (pattern, khớp_trong_message): chỉ pattern bare-literal yếu bị tắt trong
# span macro-message (xem MESSAGE_MACROS).)
SPAWN_PATTERNS = [
    # A known dynamic process wrapper is forbidden in dependency code even
    # though the executable is passed indirectly and cannot match a literal.
    # Wrapper process động đã biết bị cấm trong dependency code dù executable truyền gián tiếp.
    (re.compile(r"(?<!fn )\b(run_native_install)\s*\("), True),
    (re.compile(rf'\bexec_tool\s*\([^()]*?"({_TOOL_ALT})"'), True),
    (re.compile(rf'(?<![\w_])(?:run_inherited|run_capture|run)\s*\(\s*"({_TOOL_ALT})"'), True),
    (re.compile(rf'\bmgc_run\s*\(\s*"({_TOOL_ALT})"'), True),
    (re.compile(rf'\bCommand::new\s*\(\s*"({_TOOL_ALT})"'), True),
    (re.compile(rf'\bwhich(?:::which)?\s*\(\s*"({_TOOL_ALT})"'), True),
    # (tool, args) tables — `("cdk", vec!["deploy"])`, `("uv", vec![…])`.
    (re.compile(rf'\(\s*"({_TOOL_ALT})"\s*,\s*(?:&?\[|vec!)'), True),
    # Spawn descriptor fields — `tool: "flutter".to_string()`.
    (re.compile(rf'\btool\s*:\s*"({_TOOL_ALT})"'), True),
    # Dispatch comparisons against a TOOL VARIABLE — `tool == "uv"`,
    # `kind != "terraform"`. A bare `name == "flutter"` (dependency-name
    # iteration inside a manifest parser) is NOT a spawn and must not
    # match, so the left-hand side must name the tool being chosen.
    # (So sánh điều phối với BIẾN TOOL — `tool == "uv"`, `kind !=
    # "terraform"`. `name == "flutter"` (duyệt tên dependency trong parser
    # manifest) KHÔNG phải spawn nên không được khớp — vế trái phải là tên
    # biến đang chọn tool.)
    (re.compile(rf'\b(?:tool|cmd|bin)\s*(?:==|!=)\s*"({_TOOL_ALT})"'), True),
    (re.compile(rf'"({_TOOL_ALT})"\s*==\s*(?:tool|cmd|bin)\b'), True),
    # A bare tool literal alone on its line (variable assignment / call arg).
    (re.compile(rf'^\s*"({_TOOL_ALT})",?\s*$', re.MULTILINE), False),
]


def _message_spans(text: str) -> list:
    """Paren spans of message-macro invocations (`format!(…)`, …).
    Heuristic by design: plain `"…"` strings (with backslash escapes) are
    tracked; raw strings (`r#"…"#`) are NOT — a raw string carrying
    unbalanced parens can only ever size a span wrong, and only the weak
    bare-literal pattern consults these spans, never the call-shaped ones.
    (Span ngoặc của lời gọi macro-message. Cố ý heuristic: chỉ theo dõi
    chuỗi thường (escape backslash); chuỗi raw không — literal raw mang
    ngoặc lệch chỉ làm sai kích thước span, mà chỉ pattern bare-literal
    yếu mới dùng các span này, pattern dạng-call không bao giờ dùng.)"""
    spans = []
    for macro in MESSAGE_MACROS:
        needle = macro + "("
        search_from = 0
        while True:
            call = text.find(needle, search_from)
            if call < 0:
                break
            depth = 0
            in_string = None
            escaped = False
            cursor = call + len(needle) - 1  # at the opening paren
            end = len(text)
            while cursor < len(text):
                char = text[cursor]
                if in_string is not None:
                    if escaped:
                        escaped = False
                    elif char == "\\":
                        escaped = True
                    elif char == in_string:
                        in_string = None
                elif char in ("\"", "'"):
                    in_string = char
                elif char == "(":
                    depth += 1
                elif char == ")":
                    depth -= 1
                    if depth <= 0:
                        end = cursor + 1
                        break
                cursor += 1
            spans.append((call, end))
            search_from = call + len(needle)
    return spans


def _inside_spans(spans: list, offset: int) -> bool:
    """True when the text offset sits inside any span.
    (True khi offset text nằm trong span bất kỳ.)"""
    return any(start <= offset < end for start, end in spans)


# Top-level deny/allow-list declarations (`const X: &[&str] = &[…];`)
# name tools without spawning them — a bare literal there is a label, not
# a spawn. Only the weak bare-literal pattern consults these spans, and
# only OUTSIDE function bodies (a `const` initializer can never execute).
# Residual risk (documented): a deny-list-shaped array iterated by a
# dynamic spawn loop (`for t in TOOLS { run(t) }`) would also be skipped —
# but that loop carries no tool literal for ANY pattern to catch, so
# nothing measurable is lost; call-shaped spawns stay fully active.
# (Khai báo deny/allow-list top-level (`const X: &[&str]`) chỉ nêu tên
# tool, không spawn. Chỉ pattern bare-literal yếu dùng các span này, và
# chỉ NGOÀI thân hàm.)
CONST_ARRAY_RE = re.compile(r"^\s*const\s+[A-Za-z_][A-Za-z0-9_]*\s*:[^=;]*=\s*&\[\s*$")


def _const_array_spans(lines: list) -> list:
    """(start_offset, end_offset) of multi-line top-level `&[&str]` const
    initializers, measured in text offsets.
    (Span các khởi tạo const array nhiều dòng top-level, đo bằng offset.)"""
    text = "\n".join(lines)
    line_starts = [0]
    for line in lines:
        line_starts.append(line_starts[-1] + len(line) + 1)
    spans = []
    idx = 0
    while idx < len(lines):
        if CONST_ARRAY_RE.match(lines[idx]):
            depth = 0
            cursor = idx
            started = False
            while cursor < len(lines):
                depth += lines[cursor].count("[") - lines[cursor].count("]")
                if "[" in lines[cursor]:
                    started = True
                if started and depth <= 0:
                    spans.append((line_starts[idx], line_starts[cursor + 1]))
                    break
                cursor += 1
            idx = cursor + 1
        else:
            idx += 1
    return spans

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
        if os.path.isfile(base) and base.endswith(".rs"):
            files.append(base)
            continue
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


def _classify(rel_path: str, fn_name: str):
    """(op_class, status) for one finding.
    ((op_class, status) cho một finding.)"""
    parts = rel_path.replace(os.sep, "/").split("/")
    path_parts = [p.lower() for p in parts[:-1]]
    fn_lower = fn_name.lower()
    fn_parts = [p for p in fn_lower.split("_") if p]
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

    # 2) Function-level dependency scope wins over function-level whitelist:
    #    `run_install` is still a dependency operation. File location alone
    #    must not turn a dev/runtime helper in an install module into an
    #    install spawn; such helpers are classified by their own function.
    #    (Tên hàm dependency thắng whitelist; path install không làm helper
    #    dev/runtime thành install nếu spawn nằm trong hàm riêng.)
    for op in DEPENDENCY_OPS:
        if op in fn_parts or op in fn_lower:
            # A declaration records debt; it never grants dependency ownership.
            # Khai báo ghi nhận khoản nợ; không biến spawn ngoài thành native.
            return op, "violation"

    # 3) Whitelisted operation by function name (run_test_step, flash_fw,
    #    doctor_store, build_release, dev_server…).
    #    (Operation whitelist theo tên hàm.)
    for op in ALLOWED_OPS:
        if op in fn_parts or fn_lower.startswith(op):
            return op, "allowed"

    # A top-level spawn has no function context; use the lane path as the
    # conservative scope fallback. Function-local spawns without an
    # operation classification remain unclassified blockers.
    if fn_name == "<top-level>":
        for op in DEPENDENCY_OPS:
            if op in path_parts:
                return op, "violation"

    # 4) Unclassified spawn is a blocker; a marker never grants an exception.
    #    (Spawn chưa phân loại luôn bị chặn; marker không tạo ngoại lệ.)
    return "unclassified", "violation"


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
    message_macro_spans = _message_spans(text)
    const_array_spans = _const_array_spans(lines)
    findings = []

    for pattern, matches_inside_message in SPAWN_PATTERNS:
        for match in pattern.finditer(text):
            if not matches_inside_message and _inside_spans(
                message_macro_spans, match.start(1)
            ):
                continue
            line_no = text.count("\n", 0, match.start()) + 1
            span = _enclosing_function(spans, line_no - 1)
            if (
                not matches_inside_message
                and span is None
                and _inside_spans(const_array_spans, match.start(1))
            ):
                # Top-level deny/allow-list declaration, not a spawn.
                # (Khai báo deny/allow-list top-level, không phải spawn.)
                continue
            tool = match.group(1)
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
            op_class, status = _classify(rel_path, fn_name)
            findings.append({
                "file": rel_path,
                "line": line_no,
                "tool": tool,
                "op_class": op_class,
                "function": fn_name,
                "status": status,
                "declared_delegation": MARKER in "\n".join(fn_lines) or MARKER in doc_text,
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


def check_lane_gate(repo_root: str) -> tuple:
    """Every dependency-lane module with logic references `dep_gate::`
    or declares `GATE-EXEMPT:`. Pure routers (no function definitions)
    are exempt structurally — there is no logic to gate.
    (Mọi module lane dependency có logic phải nhắc `dep_gate::` hoặc khai
    `GATE-EXEMPT:`. Router thuần (không định nghĩa hàm) được miễn theo cấu
    trúc — không có logic để gate.)
    Returns (checked_count, missing_list)."""
    checked = 0
    missing = []
    for root in LANE_GATE_ROOTS:
        base = os.path.join(repo_root, root)
        for dirpath, _dirnames, filenames in os.walk(base):
            rel_dir = os.path.relpath(dirpath, repo_root)
            segments = [p.lower() for p in rel_dir.replace(os.sep, "/").split("/")]
            if any(seg in ("test", "tests", "bench", "benches") for seg in segments):
                continue
            for name in sorted(filenames):
                if not name.endswith(".rs"):
                    continue
                abs_path = os.path.join(dirpath, name)
                rel_path = os.path.relpath(abs_path, repo_root)
                try:
                    with open(abs_path, "r", encoding="utf-8", errors="replace") as handle:
                        text = handle.read()
                except OSError:
                    continue
                if "dep_gate::" in text or "GATE-EXEMPT:" in text:
                    checked += 1
                    continue
                if not any(FN_RE.match(line) for line in text.splitlines()):
                    continue  # pure router — no logic to gate
                checked += 1
                missing.append(rel_path)
    return checked, missing


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    findings = []
    for abs_path in _iter_rust_files():
        rel_path = os.path.relpath(abs_path, repo_root)
        findings.extend(scan_file(rel_path, abs_path))

    counts = {"allowed": 0, "violation": 0}
    for finding in findings:
        counts[finding["status"]] = counts.get(finding["status"], 0) + 1

    lane_checked, lane_missing = check_lane_gate(repo_root)

    try:
        commit = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root, capture_output=True, text=True,
        ).stdout.strip()
    except OSError:
        commit = ""

    report = {
        "schema": "dependency-delegation-audit/2",
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
            "violation": counts["violation"],
            "declared_delegations": sum(
                1 for finding in findings if finding["declared_delegation"]
            ),
        },
        "lane_gate": {
            "checked": lane_checked,
            "missing": lane_missing,
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
        f"violation={counts['violation']}"
    )
    violated = [f for f in findings if f["status"] == "violation"]
    if violated:
        print(
            "DEPENDENCY OWNERSHIP BLOCKED — remove the external package-manager "
            "spawn or implement the operation natively; `DELEGATED:` does not waive it:",
            file=sys.stderr,
        )
        for finding in violated:
            declaration = " [documented debt]" if finding["declared_delegation"] else ""
            print(
                f"  [violation] {finding['file']}:{finding['line']} "
                f"{finding['tool']} ({finding['function']}){declaration}",
                file=sys.stderr,
            )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    if lane_missing:
        print(
            "LANE WITHOUT OWNERSHIP GATE — call `dep_gate::gate()` in the "
            "lane or declare `GATE-EXEMPT:` with a reason:",
            file=sys.stderr,
        )
        for rel_path in lane_missing:
            print(f"  [no-gate] {rel_path}", file=sys.stderr)
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    print(
        f"lane-gate coverage: {lane_checked} lane file(s) reference "
        "dep_gate:: or GATE-EXEMPT:"
    )
    print(f"ledger: {os.path.relpath(out_path, repo_root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
