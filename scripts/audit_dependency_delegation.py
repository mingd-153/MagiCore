#!/usr/bin/env python3
"""P0-A dependency-delegation audit gate (2026-09-16).

V1.2 audit contract: MagiCore must become a NATIVE dependency engine. Every
external dependency-lifecycle tool spawn is a blocker; documentation cannot
waive it. This gate scans dependency lifecycle code and classifies findings:

  allowed               the file is a test/bench harness, or an explicit
                        diagnostic-only operation. Production build/test/run/
                        dev/deploy/flash tool spawns are not automatically
                        accepted merely because of their function name.
  review-required       exact OS process-boundary helpers in mgc-exec (PATH
                        shim lookup, Windows command wrapper, process-tree
                        termination/table inspection). These remain blocking;
                        they are not counted as package-manager delegations.
  violation             every dependency-lifecycle spawn, whether or not
                        it carries a `DELEGATED:` marker — the gate FAILS.
                        A marker is evidence/ledger metadata, never a waiver.

Output: gitignored JSON ledger at docs/specs/dependencyDelegationAudit.json
(Rust process findings plus Python stdlib process-call inventory) so native
engine work can be driven from source locations. Exit code 1 when a Rust
blocker, unreviewed Python call, or known coverage gap exists; this gate is
not a repository-complete release proof.

Heuristics (documented, deliberately conservative): Rust uses line/context
matching and a naive brace walk; Python uses AST for selected standard-library
process APIs. Neither is a complete language parser/dataflow analysis. Spawn
shapes include exec_tool(), mgc_exec run wrappers, Command::new(), which(),
tool descriptors and dynamic process APIs; unresolved patterns remain blockers.

Cổng ownership native: marker `DELEGATED:` chỉ ghi nhận debt, không miễn
trừ. Finding Python chưa review và bề mặt chưa quét giữ gate đỏ; ledger không
được dùng để tuyên bố đã audit toàn repository.
"""

import ast
import datetime
import json
import os
import re
import shlex
import subprocess
import sys

# Source roots covered by each language-aware pass. Rust roots include
# first-party adapters, CLI, crates, tests, and tools; Python roots are listed
# separately so new entry points cannot hide in only one implementation tree.
# (Root source được quét theo từng parser ngôn ngữ. Rust gồm adapter, CLI,
# crates, tests và tools; Python khai riêng để entry point không lọt vì chỉ
# quét một cây implementation.)
SCAN_ROOTS = [
    "adapters",
    # Scan all first-party Rust production and test source roots; selecting
    # only adapters or command subtrees could miss a new crate/entry point.
    # (Quét mọi source Rust first-party và test; chọn vài subtree có thể bỏ
    # sót crate hay entry point mới.)
    "cli",
    "core",
    "tests",
    "tools",
]
PYTHON_SCAN_ROOTS = ["."]
SHELL_SCAN_ROOTS = ["."]

# Known blind spots remain explicit, and an otherwise-clean run fails until
# these surfaces have dedicated parsers/review. (Ghi rõ điểm mù; nếu các bề
# mặt này chưa có parser/review thì kết quả sạch vẫn phải fail.)
UNSCANNED_PROCESS_SURFACES = [
    {
        "path": "Python indirect aliases and third-party process wrappers",
        "reason": "AST inventory detects direct stdlib calls but does not perform dataflow or wrapper resolution",
    },
    {
        "path": "PowerShell and JavaScript indirect/computed process calls",
        "reason": "PowerShell detects direct cmdlets and common built-in process aliases but remains lexical and does not resolve arbitrary user-defined aliases/data-flow; JavaScript covers common import/computed/static aliases but not arbitrary alias/data-flow, template-literal expressions, reflective or module-loader wrappers",
    },
    {
        "path": "shell wrappers and non-listed executable commands",
        "reason": "the shell pass inventories direct known-tool command heads only; it does not resolve sourced files, functions, eval, or arbitrary executables",
    },
    {
        "path": ".github/workflows",
        "reason": "immutable action-reference syntax has a dedicated CI gate; command provisioning and upstream SHA provenance remain incomplete",
    },
]

PYTHON_PROCESS_FUNCTIONS = {
    "subprocess": {
        "call", "check_call", "check_output", "getoutput", "getstatusoutput",
        "Popen", "run",
    },
    "os": {"fork", "popen", "posix_spawn", "posix_spawnp", "startfile", "system"},
    "pty": {"spawn"},
    "asyncio": {"create_subprocess_exec", "create_subprocess_shell"},
}

JS_PROCESS_APIS = {"exec", "execFile", "execFileSync", "execSync", "fork", "spawn", "spawnSync"}
PS_PROCESS_APIS = ("Start-Process", "Start-Job", "Invoke-Expression", "Invoke-Command")
# Built-in PowerShell aliases that can evaluate or start commands. Keep aliases
# separate so findings identify the underlying process boundary, not a tool name.
# Alias PowerShell dựng sẵn có thể chạy hoặc đánh giá lệnh; ánh xạ về cmdlet gốc.
PS_PROCESS_ALIASES = {
    "start": "Start-Process",
    "saps": "Start-Process",
    "iex": "Invoke-Expression",
    "icm": "Invoke-Command",
    "sajb": "Start-Job",
}

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

# Diagnostic probes may inspect installed tools, but product workflows may
# not silently hand build/test/run/dev/deploy/flash ownership to a CLI.
# Test and benchmark source paths are handled separately below.
# (Chỉ cho phép probe chẩn đoán; workflow production không được tự động
# giao quyền build/test/run/dev/deploy/flash cho CLI ngoài.)
ALLOWED_OPS = ("doctor",)
ECOSYSTEM_OWNED_OPS = ("build", "test", "run", "dev", "flash", "deploy")

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

# Internal routing sentinels are values, not executable names. Keep the set
# exact so it cannot become a general bypass for unlisted process targets.
# (Sentinel điều phối nội bộ không phải tên executable; allowlist chính xác.)
NON_EXECUTABLE_SENTINELS = {"mgc-internal-run-script"}
NON_EXECUTABLE_SQL_PREFIXES = {
    "ALTER", "CREATE", "DELETE", "DROP", "INSERT", "PRAGMA", "SELECT",
    "UPDATE", "WITH",
}

# Exact first-party OS process helpers are kept distinct from ecosystem tool
# delegation. They remain blocking `review-required` findings: this exception
# does not approve the implementation or claim native behavior, it only keeps
# an OS process-control primitive from being mislabeled as a package manager.
# Exact helper/path/tool matching is intentional; dynamic executables and any
# package/build tool launched from the same function still use normal rules.
# (Các helper OS được phân loại riêng và vẫn blocking; không miễn executable
# động hoặc package/build tool gọi từ cùng hàm.)
PLATFORM_PROCESS_BOUNDARY_REVIEWS = {
    (
        "core/crates/mgc-exec/src/run.rs",
        "execute_command",
        "cmd.exe",
    ): "Windows batch-script command wrapper; review shell-boundary safety",
    (
        "core/crates/mgc-exec/src/run.rs",
        "terminate_process_tree",
        "taskkill",
    ): "Windows process-tree termination; review native Job Object replacement",
    (
        "core/crates/mgc-exec/src/run.rs",
        "find_forbidden_descendant",
        "ps",
    ): "Unix process-table inspection; review native platform API replacement",
}

# These two first-party toolchain calls are explicitly constrained by
# `mgc-exec`: Cargo is locked/offline and rustc is limited to the read-only
# host-target query. Keep the exception exact by source, function, and tool.
# (Hai lệnh toolchain nội bộ này bị giới hạn bởi `mgc-exec`: Cargo khóa
# dependency/offline, rustc chỉ truy vấn host-target chỉ đọc. Ngoại lệ phải
# khớp chính xác file, hàm và executable.)
# The web lifecycle route is user-policy gated, parsed as one non-shell
# invocation, and constrained to the install execution scope.
# Route lifecycle web bị policy user chặn/mở, parse thành lệnh đơn không shell,
# và dùng phạm vi thực thi Install.
# Publish lifecycle execution requires a separate explicit one-command opt-in.
# Hook publish cần opt-in tường minh riêng cho từng lần chạy.
# Publish's Git checks accept only the closed read-only query enum and run
# against the selected project root; dynamic refs follow --end-of-options.
# Git check của publish chỉ nhận enum truy vấn đóng, chỉ đọc, chạy tại root
# project được chọn; ref động nằm sau --end-of-options.
AUDITED_EXECUTOR_ROUTES = {
    (
        "tools/mgc-dist/src/main.rs",
        "build_package",
        "cargo",
    ): "mgc-exec BuildRunner with --locked --offline; package build route",
    (
        "tools/mgc-dist/src/main.rs",
        "detect_host_target",
        "rustc",
    ): "mgc-exec BuildRunner restricted to rustc -vV; read-only host query",
    (
        "cli/src/commands/build/web_engine.rs",
        "build_rust_with_env",
        "cargo",
    ): "mgc-exec BuildRunner; Cargo build requires --locked --offline",
    (
        "cli/src/commands/build.rs",
        "build_lib",
        "node",
    ): "mgc-exec BuildRunner; node executes the project-local tsc entry",
    (
        "cli/src/commands/build.rs",
        "build_cloud",
        "node",
    ): "mgc-exec BuildRunner; node executes the project-local CDK synth entry",
    (
        "cli/src/commands/core/dev/app.rs",
        "run_xcrun",
        "xcrun",
    ): "mgc-exec DeviceControl; allowlisted iOS simulator/device commands",
    (
        "cli/src/commands/core/dev/app.rs",
        "run_android_device_command",
        "adb",
    ): "mgc-exec DeviceControl; allowlisted Android device commands",
    (
        "adapters/web/src/lifecycle.rs",
        "run_script",
        "<dynamic:invocation>",
    ): "mgc-exec Install; parsed non-shell script from trust-approved lifecycle install",
    (
        "cli/src/commands/publish.rs",
        "run_lifecycle",
        "<dynamic:invocation>",
    ): "mgc-exec Install; non-shell lifecycle runner after explicit --allow-scripts",
    (
        "cli/src/commands/publish.rs",
        "run_git_capture",
        "git",
    ): "mgc-exec; closed read-only Git query enum at the selected project root",
}

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
    # Capture every literal executable, not only names in TOOLS. TOOLS is
    # an inventory for dispatch/string-table shapes, not a scanner boundary.
    # (Bắt mọi executable literal; TOOLS chỉ dùng cho dạng dispatch/bảng.)
    (re.compile(r'\bexec_tool\s*\([^()]*?"([^"\n]+)"'), True),
    # `run` is too common for a bare-name match: CLI command dispatchers also
    # expose `run(...)`. Match only the known process wrappers when unqualified,
    # and the explicit mgc_exec namespace for its generic `run` wrapper.
    (re.compile(r'\bmgc_exec(?:::[A-Za-z_][A-Za-z0-9_]*)*::run\s*\(\s*"([^"\n]+)"'), True),
    (re.compile(r'\bmgc_exec(?:::[A-Za-z_][A-Za-z0-9_]*)*::run_inherited\s*\(\s*"([^"\n]+)"'), True),
    (re.compile(r'(?<![\w:])(?:run_inherited|run_capture)\s*\(\s*"([^"\n]+)"'), True),
    (re.compile(r'\bmgc_run\s*\(\s*"([^"\n]+)"'), True),
    (re.compile(r'\bCommand::new\s*\(\s*"([^"\n]+)"'), True),
    (re.compile(r'\bwhich(?:::which)?\s*\(\s*"([^"\n]+)"'), True),
    # (tool, args) tables — `("cdk", vec!["deploy"])`, `("uv", vec![…])`.
    (re.compile(r'\(\s*"([^"\n]+)"\s*,\s*(?:&?\[|vec!)'), True),
    # Spawn descriptor fields — `tool: "flutter".to_string()`.
    (re.compile(r'\btool\s*:\s*"([^"\n]+)"'), True),
    # A bare tool literal alone on its line (variable assignment / call arg).
    (re.compile(rf'^\s*"({_TOOL_ALT})",\s*$', re.MULTILINE), False),
]

# These are static command-route declarations rather than proof that the
# current line executes a process. Keep them in the blocking ledger so the
# route cannot disappear from review, but label them separately from spawn
# calls. (Descriptor là tuyến lệnh tĩnh, không phải bằng chứng process chạy.)
DESCRIPTOR_PATTERNS = [pattern for pattern, _ in SPAWN_PATTERNS[8:]]

# A variable executable is still a process boundary. Keep these separate so
# the ledger labels it as dynamic instead of pretending the variable name is
# the actual program. Fixed structural wrappers are covered; arbitrary
# method-call expressions remain a documented scanner limitation.
# (Executable qua biến vẫn là process boundary; ledger phải gắn nhãn dynamic.)
DYNAMIC_SPAWN_PATTERNS = [
    re.compile(r'(?<!fn )\bexec_tool\s*\(\s*&?([A-Za-z_][A-Za-z0-9_]*)'),
    re.compile(r'\bmgc_run\s*\(\s*&?([A-Za-z_][A-Za-z0-9_]*)'),
    re.compile(
        r'(?<!fn )\bmgc_exec(?:::[A-Za-z_][A-Za-z0-9_]*)*::exec_tool'
        r'\s*\(\s*&?([A-Za-z_][A-Za-z0-9_]*)'
    ),
    re.compile(r'\bCommand::new\s*\(\s*&?([A-Za-z_][A-Za-z0-9_]*)'),
    re.compile(
        r'\bmgc_exec(?:::[A-Za-z_][A-Za-z0-9_]*)*::'
        r'(?:run|run_inherited|run_capture)\s*\(\s*&?([A-Za-z_][A-Za-z0-9_]*)'
    ),
    re.compile(
        r'(?<![\w:])(?:run_inherited|run_capture)\s*\(\s*&?([A-Za-z_][A-Za-z0-9_]*)'
    ),
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
CONST_ARRAY_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+[A-Za-z_][A-Za-z0-9_]*\s*:[^=;]*=\s*&\[\s*$"
)


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
        if not os.path.isdir(base):
            raise FileNotFoundError(f"required Rust scan root is missing: {root}")

        def raise_walk_error(error):
            raise error

        for dirpath, dirnames, filenames in os.walk(base, onerror=raise_walk_error):
            dirnames[:] = sorted(
                name for name in dirnames
                if name not in {".git", ".venv", "deps", "node_modules", "target", "vendor"}
            )
            for name in sorted(filenames):
                if name.endswith(".rs"):
                    files.append(os.path.join(dirpath, name))
    return sorted(files)


def _iter_python_files() -> list:
    """Every first-party Python source under audited source roots.
    (Mọi source Python first-party trong các root được kiểm toán.)"""
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    files = []
    for root in PYTHON_SCAN_ROOTS:
        base = os.path.join(repo_root, root)
        if not os.path.isdir(base):
            raise FileNotFoundError(f"required Python scan root is missing: {root}")

        def raise_walk_error(error):
            raise error

        for dirpath, dirnames, filenames in os.walk(base, onerror=raise_walk_error):
            dirnames[:] = sorted(
                name for name in dirnames
                if name not in {
                    ".git", ".gitnexus", ".kilo", "__pycache__", ".venv", ".pytest_cache",
                    "build", "deps", "dist", "node_modules", "target", "vendor",
                }
            )
            for name in sorted(filenames):
                if name.endswith(".py") and "gitnexus" not in name.lower():
                    files.append(os.path.join(dirpath, name))
    return sorted(files)


def scan_python_file(abs_path: str, repo_root: str) -> list:
    """Read and inventory one Python file, failing closed on read errors.
    (Đọc và kiểm kê một file Python; lỗi đọc được giữ thành finding.)"""
    rel_path = os.path.relpath(abs_path, repo_root).replace(os.sep, "/")
    try:
        with open(abs_path, "r", encoding="utf-8") as handle:
            return scan_python_text(rel_path, handle.read())
    except (OSError, UnicodeError) as error:
        return [{
            "file": rel_path,
            "line": 1,
            "api": "<read-error>",
            "executable": "<unreadable-python>",
            "review_status": "unreviewed",
            "parse_error": str(error),
        }]


SHELL_WRAPPERS = {
    "command", "env", "exec", "nice", "nohup", "sudo", "time", "timeout",
}
SHELL_DYNAMIC_EXECUTORS = {".", "eval", "source"}
SHELL_INTERPRETERS = {"bash", "dash", "powershell", "pwsh", "sh", "zsh"}
SHELL_WRAPPER_VALUE_OPTIONS = {
    "env": {"-u", "--unset", "-C", "--chdir", "-S", "--split-string"},
    "nice": {"-n", "--adjustment"},
    "sudo": {"-u", "--user", "-g", "--group", "-C", "--chdir", "-h", "--host"},
    "time": {"-o", "--output", "-f", "--format"},
    "timeout": {"-k", "--kill-after", "-s", "--signal"},
}
SHELL_WRAPPER_POSITIONAL_COUNTS = {"timeout": 1}
SHELL_CONTROL_WORDS = {
    "!", "{", "}", "do", "done", "elif", "else", "fi", "if", "then",
    "until", "while",
}
SHELL_ASSIGNMENT_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


def _shell_command_segments(text: str):
    """Yield logical shell command segments with their first physical line.
    (Tách command shell theo control operator, giữ dòng bắt đầu để audit.)"""
    pending = ""
    start_line = 1
    pending_continuation = False
    for line_no, physical in enumerate(text.splitlines(), start=1):
        if not pending:
            start_line = line_no
        stripped = physical.rstrip()
        continued = stripped.endswith("\\")
        pending_continuation = continued
        piece = stripped[:-1] if continued else physical
        pending += piece + (" " if continued else "\n")
        if continued:
            continue
        try:
            lexer = shlex.shlex(pending, posix=True, punctuation_chars=";&|")
            lexer.whitespace_split = True
            lexer.commenters = "#"
            tokens = list(lexer)
        except ValueError as error:
            if "No closing quotation" in str(error):
                # Shell single/double-quoted strings may legally span physical
                # lines; retain the source until shlex can parse the complete
                # command instead of turning valid multiline input red.
                # (Quote shell hợp lệ có thể qua nhiều dòng; giữ input đến khi
                # parse được cả command, không biến cú pháp đúng thành lỗi.)
                continue
            yield start_line, None, str(error)
            pending = ""
            continue
        segment = []
        for token in tokens:
            if token and all(char in ";&|" for char in token):
                if segment:
                    yield start_line, segment, None
                    segment = []
            else:
                segment.append(token)
        if segment:
            yield start_line, segment, None
        pending = ""
        pending_continuation = False
    if pending:
        if pending_continuation:
            yield start_line, None, "line continuation reaches end of file"
            return
        try:
            lexer = shlex.shlex(pending, posix=True, punctuation_chars=";&|")
            lexer.whitespace_split = True
            lexer.commenters = "#"
            tokens = list(lexer)
        except ValueError as error:
            yield start_line, None, str(error)
            return
        segment = []
        for token in tokens:
            if token and all(char in ";&|" for char in token):
                if segment:
                    yield start_line, segment, None
                    segment = []
            else:
                segment.append(token)
        if segment:
            yield start_line, segment, None


def _shell_executable(segment: list):
    """Resolve a direct known-tool command after common transparent wrappers.
    (Tìm executable thuộc inventory sau wrapper shell phổ biến.)"""
    index = 0
    nested_known_tool = re.compile(
        r"(?:\$\(|`)\s*(?:[^\s;&|]*/)?(?:"
        + "|".join(re.escape(tool) for tool in sorted(TOOLS, key=len, reverse=True))
        + r")\b"
    )
    while index < len(segment):
        token = segment[index]
        lowered = token.lower()
        if lowered in SHELL_CONTROL_WORDS or SHELL_ASSIGNMENT_RE.match(token):
            index += 1
            continue
        if lowered in SHELL_WRAPPERS:
            wrapper = lowered
            index += 1
            positional_remaining = SHELL_WRAPPER_POSITIONAL_COUNTS.get(wrapper, 0)
            while index < len(segment):
                argument = segment[index]
                if SHELL_ASSIGNMENT_RE.match(argument):
                    index += 1
                    continue
                if argument.startswith("-"):
                    index += 1
                    if (
                        argument in SHELL_WRAPPER_VALUE_OPTIONS.get(wrapper, set())
                        and index < len(segment)
                    ):
                        index += 1
                    continue
                if positional_remaining:
                    positional_remaining -= 1
                    index += 1
                    continue
                break
            continue
        # Only the command-position token determines the executable. Variables
        # and substitutions in ordinary arguments do not turn a static
        # command into a dynamic executable (e.g. `echo "$HOME"`).
        # (Chỉ token vị trí command quyết định executable; biến trong đối số
        # không biến lệnh tĩnh thành executable động.)
        if (
            token.startswith("$")
            or "$" in token
            or any(marker in token for marker in ("$(", "`", "<(", ">("))
            or token in {"(", ")"}
        ):
            return "<dynamic-shell-command>"
        basename = token.replace("\\", "/").rsplit("/", 1)[-1]
        if basename in SHELL_DYNAMIC_EXECUTORS or basename in SHELL_INTERPRETERS:
            return "<dynamic-shell-command>"
        if basename in TOOLS:
            return basename
        break
    # Command substitutions execute even in assignment-only statements. Detect
    # known PM heads inside those substitutions without treating unrelated
    # variables in ordinary arguments as dynamic executable names.
    # (Command substitution vẫn chạy trong assignment-only; bắt PM đã biết
    # bên trong nhưng không gắn nhãn động cho biến ở đối số thông thường.)
    if any(nested_known_tool.search(token) for token in segment):
        return "<dynamic-shell-command>"
    return None


def scan_shell_text(rel_path: str, text: str) -> list:
    """Inventory direct known-tool shell commands; malformed input is unreviewed.
    (Kiểm kê command gọi tool đã biết; cú pháp lỗi không được xem là sạch.)"""
    findings = []
    for line_no, segment, parse_error in _shell_command_segments(text):
        if parse_error is not None:
            findings.append({
                "file": rel_path,
                "line": line_no,
                "api": "shell-command",
                "executable": "<unparsed-shell>",
                "review_status": "unreviewed",
                "parse_error": parse_error,
            })
            continue
        executable = _shell_executable(segment)
        if executable is None:
            continue
        findings.append({
            "file": rel_path,
            "line": line_no,
            "api": "shell-command",
            "executable": executable,
            "review_status": "unreviewed",
            "parse_error": None,
        })
    return findings


def _iter_shell_files() -> list:
    """Walk shell sources in first-party and workflow trees deterministically.
    (Quét file shell trong cây first-party/workflow, bỏ build/cache/vendor.)"""
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    excluded = {
        ".git", ".gitnexus", ".kilo", ".venv", "__pycache__", ".pytest_cache",
        "build", "deps", "dist", "node_modules", "target", "vendor",
    }
    files = []
    for root in SHELL_SCAN_ROOTS:
        base = os.path.join(repo_root, root)
        if not os.path.isdir(base):
            raise FileNotFoundError(f"required shell scan root is missing: {root}")

        def raise_walk_error(error):
            raise error

        for dirpath, dirnames, filenames in os.walk(base, onerror=raise_walk_error):
            dirnames[:] = sorted(name for name in dirnames if name not in excluded)
            for filename in sorted(filenames):
                if (
                    filename.endswith((".sh", ".bash", ".zsh"))
                    and "gitnexus" not in filename.lower()
                ):
                    files.append(os.path.join(dirpath, filename))
    return sorted(set(files))


def scan_shell_file(abs_path: str, repo_root: str) -> list:
    """Read and inventory one shell file, failing closed on read errors.
    (Đọc và kiểm kê file shell; lỗi đọc phải xuất hiện trong ledger.)"""
    rel_path = os.path.relpath(abs_path, repo_root).replace(os.sep, "/")
    try:
        with open(abs_path, "r", encoding="utf-8") as handle:
            return scan_shell_text(rel_path, handle.read())
    except (OSError, UnicodeError) as error:
        return [{
            "file": rel_path,
            "line": 1,
            "api": "<read-error>",
            "executable": "<unreadable-shell>",
            "review_status": "unreviewed",
            "parse_error": str(error),
        }]


def _mask_js_comments(text: str) -> str:
    """Blank JS comments while preserving strings and physical line numbers.
    (Xóa comment JS nhưng giữ string và số dòng vật lý.)"""
    out = list(text)
    index = 0
    quote = None
    escaped = False
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            index += 1
            continue
        if char in ("'", '"', "`"):
            quote = char
            index += 1
        elif char == "/" and next_char == "/":
            while index < len(text) and text[index] != "\n":
                out[index] = " "
                index += 1
        elif char == "/" and next_char == "*":
            out[index] = out[index + 1] = " "
            index += 2
            while index < len(text):
                if text[index] == "\n":
                    index += 1
                    continue
                if text[index] == "*" and index + 1 < len(text) and text[index + 1] == "/":
                    out[index] = out[index + 1] = " "
                    index += 2
                    break
                out[index] = " "
                index += 1
        else:
            index += 1
    return "".join(out)


def _mask_js_strings(text: str) -> str:
    """Blank JS string/template bodies while preserving newlines and code.
    (Xóa nội dung string/template JS nhưng giữ newline và phần code.)"""
    out = list(text)
    index = 0
    quote = None
    escaped = False
    while index < len(text):
        char = text[index]
        if quote:
            if char == "\n" and quote != "`":
                quote = None
                index += 1
                continue
            if char != "\n":
                out[index] = " "
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
        elif char in ("'", '"', "`"):
            quote = char
            out[index] = " "
        index += 1
    return "".join(out)


def scan_javascript_text(rel_path: str, text: str) -> list:
    """Conservatively inventory common JS/Bun/Deno process APIs.
    (Kiểm kê bảo thủ API process JS/Bun/Deno thường gặp.)"""
    source = _mask_js_comments(text)
    code = _mask_js_strings(source)
    findings = []
    aliases = {}
    namespaces = set()
    import_re = re.compile(
        r"import\s*\{([^}]+)\}\s*from\s*(['\"])(?:node:)?child_process\2"
        r"|require\s*\(\s*(['\"])(?:node:)?child_process\3\s*\)"
    )
    for match in import_re.finditer(source):
        imported = match.group(1)
        if imported:
            for item in imported.split(","):
                parts = re.split(r"\s+as\s+", item.strip())
                name = parts[0].strip()
                alias = parts[-1].strip()
                if name in JS_PROCESS_APIS:
                    aliases[alias] = f"child_process.{name}"
        else:
            line_start = code.rfind("\n", 0, match.start()) + 1
            prefix = code[line_start:match.start()]
            assigned = re.search(r"(?:const|let|var)\s+(\w+)\s*=\s*$", prefix)
            if assigned:
                namespaces.add(assigned.group(1))
    commonjs_destructure = re.compile(
        r"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*require\s*\(\s*(['\"])(?:node:)?child_process\2\s*\)"
    )
    for match in commonjs_destructure.finditer(source):
        for item in match.group(1).split(","):
            parts = re.split(r"\s*:\s*", item.strip())
            name = parts[0].strip()
            alias = parts[-1].strip()
            if name in JS_PROCESS_APIS:
                aliases[alias] = f"child_process.{name}"
    namespace_import = re.compile(
        r"import\s+(?:\*\s+as\s+)?([A-Za-z_$][\w$]*)\s+from\s*(['\"])(?:node:)?child_process\2"
    )
    for match in namespace_import.finditer(source):
        namespaces.add(match.group(1))

    dynamic_namespace_import = re.compile(
        r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:await\s*)?"
        r"import\s*\(\s*(['\"])(?:node:)?child_process\2\s*\)"
    )
    for match in dynamic_namespace_import.finditer(source):
        namespaces.add(match.group(1))

    dynamic_destructure_import = re.compile(
        r"(?:const|let|var)\s*\{([^}]+)\}\s*=\s*(?:await\s*)?"
        r"import\s*\(\s*(['\"])(?:node:)?child_process\2\s*\)"
    )
    for match in dynamic_destructure_import.finditer(source):
        for item in match.group(1).split(","):
            parts = re.split(r"\s*:\s*", item.strip(), maxsplit=1)
            imported = parts[0].strip()
            alias = parts[-1].strip()
            if imported in JS_PROCESS_APIS:
                aliases[alias] = f"child_process.{imported}"

    # Follow simple literal namespace copies, but do not claim arbitrary data-flow coverage.
    # Theo alias namespace dạng literal; không tuyên bố đã bao phủ data-flow tổng quát.
    namespace_alias = re.compile(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*)\s*;?"
    )
    while True:
        added = {
            match.group(1)
            for match in namespace_alias.finditer(code)
            if match.group(2) in namespaces and match.group(1) not in namespaces
        }
        if not added:
            break
        namespaces.update(added)

    computed_calls = []
    for namespace in namespaces:
        computed_member = re.compile(
            rf"\b{re.escape(namespace)}\s*(?:\?\.|\.)?\s*"
            r"\[\s*(['\"])(" + "|".join(sorted(JS_PROCESS_APIS)) + r")\1\s*\]"
        )
        for match in computed_member.finditer(source):
            api_name = match.group(2)
            if re.match(r"\s*\(", code[match.end():]):
                computed_calls.append((match.start(), api_name))
            line_start = code.rfind("\n", 0, match.start()) + 1
            prefix = code[line_start:match.start()]
            assigned = re.search(r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*$", prefix)
            if assigned:
                aliases[assigned.group(1)] = f"child_process.{api_name}"

    patterns = [(re.compile(rf"\b{re.escape(alias)}\s*\("), api) for alias, api in aliases.items()]
    for namespace in namespaces:
        patterns.extend(
            (
                re.compile(rf"\b{re.escape(namespace)}\s*(?:\?\.|\.)\s*{api}\s*\("),
                f"child_process.{api}",
            )
            for api in JS_PROCESS_APIS
        )
        patterns.extend(
            (
                re.compile(
                    rf"\b{re.escape(namespace)}\s*(?:\?\.|\.)?\s*"
                    rf"\[\s*(['\"]){api}\1\s*\]\s*\("
                ),
                f"child_process.{api}",
            )
            for api in JS_PROCESS_APIS
        )
    patterns.extend([
        (re.compile(r"\bBun\s*\.\s*spawn\s*\("), "Bun.spawn"),
        (re.compile(r"\bDeno\s*\.\s*(?:Command|run)\s*\("), "Deno.Command"),
        (re.compile(r"\beval\s*\("), "eval"),
        (re.compile(r"\bnew\s+Function\s*\("), "Function"),
    ])
    seen = set()
    inline_require = re.compile(
        r"require\s*\(\s*(['\"])(?:node:)?child_process\1\s*\)\s*\.\s*(" + "|".join(sorted(JS_PROCESS_APIS)) + r")\s*\("
    )
    for match in inline_require.finditer(source):
        line = source.count("\n", 0, match.start()) + 1
        api = f"child_process.{match.group(2)}"
        seen.add((line, api))
        findings.append({
            "file": rel_path,
            "line": line,
            "api": api,
            "executable": "<dynamic-or-unreviewed>",
            "review_status": "unreviewed",
        })
    for pattern, api in patterns:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            key = (line, api)
            if key in seen:
                continue
            seen.add(key)
            findings.append({
                "file": rel_path,
                "line": line,
                "api": api,
                "executable": "<dynamic-or-unreviewed>",
                "review_status": "unreviewed",
            })
    for offset, api_name in computed_calls:
        line = source.count("\n", 0, offset) + 1
        key = (line, f"child_process.{api_name}")
        if key in seen:
            continue
        seen.add(key)
        findings.append({
            "file": rel_path,
            "line": line,
            "api": key[1],
            "executable": "<dynamic-or-unreviewed>",
            "review_status": "unreviewed",
        })
    return sorted(findings, key=lambda item: (item["line"], item["api"]))


def _mask_powershell_comments_and_strings(line: str) -> str:
    """Blank single/double-quoted values and trailing PowerShell comments.
    (Xóa chuỗi và comment cuối dòng PowerShell để tránh match ví dụ.)"""
    out = list(line)
    quote = None
    index = 0
    while index < len(line):
        char = line[index]
        if quote:
            out[index] = " "
            if char == quote:
                if quote == "'" and index + 1 < len(line) and line[index + 1] == "'":
                    out[index + 1] = " "
                    index += 2
                    continue
                if index == 0 or line[index - 1] != "`":
                    quote = None
        elif char in ("'", '"'):
            quote = char
            out[index] = " "
        elif char == "#":
            for pos in range(index, len(line)):
                out[pos] = " "
            break
        index += 1
    return "".join(out)


def scan_powershell_text(rel_path: str, text: str) -> list:
    """Inventory PowerShell process operators and known native command heads.
    (Kiểm kê toán tử process PowerShell và command native đã biết.)"""
    findings = []
    tool_alt = "|".join(re.escape(tool) for tool in sorted(TOOLS, key=len, reverse=True))
    native_re = re.compile(rf"(?:^|[|;&{{]\s*)(?:[\w./\\:-]+/)?({tool_alt})(?=\s|$)", re.IGNORECASE)
    for line_no, line in enumerate(text.splitlines(), start=1):
        code = _mask_powershell_comments_and_strings(line)
        matches = []
        for api in PS_PROCESS_APIS:
            if re.search(rf"\b{re.escape(api)}\b", code, re.IGNORECASE):
                matches.append((api, api))
        alias_alt = "|".join(re.escape(alias) for alias in sorted(PS_PROCESS_ALIASES, key=len, reverse=True))
        for match in re.finditer(
            rf"(?:^|[|;&{{}}]\s*)({alias_alt})(?=\s|$|[|;&{{}}])",
            code,
            re.IGNORECASE,
        ):
            alias = match.group(1).lower()
            matches.append(("process-alias", PS_PROCESS_ALIASES[alias]))
        if re.search(r"(?:^|\s)&\s*(?:\$|\(|\{)", code):
            matches.append(("call-operator", "<dynamic>"))
        quoted_tool = re.search(
            rf"^\s*&\s*(['\"])(?:[A-Za-z]:[\\/])?(?:[^'\"]*[\\/])?({tool_alt})\1(?=\s|$)",
            line,
            re.IGNORECASE,
        )
        if quoted_tool:
            matches.append(("call-operator", quoted_tool.group(2).lower()))
        if re.search(r"\b(?:cmd|pwsh|powershell)\s+(?:/c|-Command)\b", code, re.IGNORECASE):
            matches.append(("shell-command", "<dynamic>"))
        for match in native_re.finditer(code):
            matches.append(("native-command", match.group(1).lower()))
        for api, executable in sorted(set(matches)):
            findings.append({
                "file": rel_path,
                "line": line_no,
                "api": api,
                "executable": executable,
                "review_status": "unreviewed",
            })
    return findings


def _iter_files_with_suffixes(suffixes: tuple) -> list:
    """Enumerate first-party source files for supplemental process scans.
    (Liệt kê source first-party cho các lượt quét process bổ sung.)"""
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    excluded = {
        ".git", ".gitnexus", ".kilo", ".venv", "__pycache__", ".pytest_cache",
        "build", "deps", "dist", "node_modules", "target", "vendor",
    }
    files = []
    for dirpath, dirnames, filenames in os.walk(repo_root):
        dirnames[:] = sorted(name for name in dirnames if name not in excluded)
        for filename in sorted(filenames):
            if filename.endswith(suffixes) and "gitnexus" not in filename.lower():
                files.append(os.path.join(dirpath, filename))
    return sorted(files)


def scan_supplemental_file(abs_path: str, repo_root: str, scanner) -> list:
    """Run a supplemental scanner and preserve read failures in its ledger.
    (Chạy scanner bổ sung; lỗi đọc phải xuất hiện trong ledger.)"""
    rel_path = os.path.relpath(abs_path, repo_root).replace(os.sep, "/")
    try:
        with open(abs_path, "r", encoding="utf-8") as handle:
            return scanner(rel_path, handle.read())
    except (OSError, UnicodeError) as error:
        return [{
            "file": rel_path,
            "line": 1,
            "api": "<read-error>",
            "executable": "<unreadable-source>",
            "review_status": "unreviewed",
            "parse_error": str(error),
        }]


def _python_call_api(node: ast.Call, module_aliases: dict, function_aliases: dict):
    """Resolve known Python stdlib process API calls through import aliases.
    (Phân giải lời gọi API process stdlib qua alias import.)"""
    func = node.func
    if isinstance(func, ast.Attribute) and isinstance(func.value, ast.Name):
        module = module_aliases.get(func.value.id)
        if module in PYTHON_PROCESS_FUNCTIONS:
            name = func.attr
            if name in PYTHON_PROCESS_FUNCTIONS[module]:
                return f"{module}.{name}"
            if module == "os" and name.startswith(("exec", "spawn")):
                return f"os.{name}"
    if isinstance(func, ast.Name):
        return function_aliases.get(func.id)
    return None


def _python_executable(node: ast.Call, api: str) -> str:
    """Extract a static argv head when safe; preserve dynamic calls explicitly.
    (Lấy argv tĩnh khi an toàn; giữ nguyên nhãn cho lệnh động.)"""
    if not node.args:
        return "<dynamic>"
    try:
        argument = ast.literal_eval(node.args[0])
    except (ValueError, TypeError, SyntaxError):
        return "<dynamic>"
    if isinstance(argument, (list, tuple)):
        if not argument:
            return "<dynamic>"
        argument = argument[0]
    if not isinstance(argument, str):
        return "<dynamic>"
    shell_string = api in {
        "os.system", "os.popen", "subprocess.getoutput", "subprocess.getstatusoutput",
        "asyncio.create_subprocess_shell",
    } or any(
        keyword.arg == "shell"
        and isinstance(keyword.value, ast.Constant)
        and keyword.value.value is True
        for keyword in node.keywords
    )
    if shell_string:
        return "<shell-command>"
    return argument


def scan_python_text(rel_path: str, text: str) -> list:
    """Inventory known Python stdlib process calls; findings need review.
    (Kiểm kê lời gọi process stdlib đã biết; mọi finding cần review.)"""
    try:
        tree = ast.parse(text, filename=rel_path)
    except SyntaxError as error:
        return [{
            "file": rel_path,
            "line": error.lineno or 1,
            "api": "<parse-error>",
            "executable": "<unparsed-python>",
            "review_status": "unreviewed",
            "parse_error": error.msg,
        }]

    module_aliases = {}
    function_aliases = {}
    wildcard_imports = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                root_module = alias.name.split(".", 1)[0]
                bound_name = alias.asname or root_module
                module_aliases[bound_name] = root_module
        elif isinstance(node, ast.ImportFrom) and node.module:
            module = node.module.split(".", 1)[0]
            if module not in PYTHON_PROCESS_FUNCTIONS:
                continue
            for alias in node.names:
                if alias.name == "*":
                    wildcard_imports.append({
                        "file": rel_path,
                        "line": node.lineno,
                        "api": f"<wildcard-import:{module}>",
                        "executable": "<dynamic>",
                        "review_status": "unreviewed",
                        "parse_error": "wildcard import prevents resolving process API calls",
                    })
                    continue
                if alias.name in PYTHON_PROCESS_FUNCTIONS[module] or (
                    module == "os"
                    and alias.name.startswith(("exec", "spawn"))
                ):
                    function_aliases[alias.asname or alias.name] = f"{module}.{alias.name}"

    findings = list(wildcard_imports)
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        api = _python_call_api(node, module_aliases, function_aliases)
        if api is None:
            continue
        findings.append({
            "file": rel_path,
            "line": node.lineno,
            "api": api,
            "executable": _python_executable(node, api),
            "review_status": "unreviewed",
            "parse_error": None,
        })
    return sorted(findings, key=lambda item: (item["line"], item["api"]))


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


def _is_test_function(lines: list, start_idx: int) -> bool:
    """Whether a function has an explicit Rust test attribute.
    (Xác định hàm có thuộc tính test Rust tường minh hay không.)"""
    cursor = start_idx - 1
    while cursor >= 0:
        stripped = lines[cursor].strip()
        if not stripped or stripped.startswith("///") or stripped.startswith("//!"):
            cursor -= 1
            continue
        if stripped.startswith("#["):
            if re.match(r"#\[(?:tokio::)?test(?:\s*\(|\s*\])", stripped):
                return True
            cursor -= 1
            continue
        break
    return False


def _is_external_harness_path(path_parts: list) -> bool:
    """Only non-`src` integration/bench roots get a path-level exemption.
    (Chỉ root integration/bench ngoài `src` mới được miễn theo đường dẫn.)"""
    for idx, segment in enumerate(path_parts):
        if segment in {"tests", "bench", "benches"} and "src" not in path_parts[:idx]:
            return True
    return False


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

    # Exact non-mutating OS/tool-version probes owned by `mgc doctor`.
    # This exception is path+symbol scoped; a directory name alone is not
    # enough to waive arbitrary external execution.
    # (Chỉ probe OS/version chính xác của doctor; không miễn cả directory.)
    if rel_path.replace(os.sep, "/") == "cli/src/commands/doctor.rs" and fn_name in {
        "tool_version",
        "fs_avail",
    }:
        return "doctor", "allowed"

    # Only integration/bench roots outside any `src` tree are path-exempt.
    # Singular `test` and source subdirectories named `tests` are not.
    # (Chỉ integration/bench root ngoài mọi cây `src` mới được miễn theo
    # path; `test` đơn lẻ và `src/tests` không được miễn.)
    if _is_external_harness_path(path_parts):
        return next(
            segment for segment in path_parts
            if segment in {"tests", "bench", "benches"}
        ), "allowed"

    # Function-level dependency scope wins over runtime/deploy paths:
    # `run_install` inside a dev module remains a dependency operation.
    # Conversely, `dev_command` inside an install module stays a runtime
    # boundary and is classified by its own function.
    # (Tên hàm lifecycle thắng path runtime/deploy; helper runtime riêng
    # trong module install vẫn theo tên hàm của chính nó.)
    for op in DEPENDENCY_OPS:
        if op in fn_parts or op in fn_lower:
            # A declaration records debt; it never grants dependency ownership.
            # Khai báo ghi nhận khoản nợ; không biến spawn ngoài thành native.
            return op, "violation"

    # A top-level spawn has no function context; a dependency path is the
    # conservative fallback, never a runtime exception.
    if fn_name == "<top-level>":
        for op in DEPENDENCY_OPS:
            if op in path_parts:
                return op, "violation"

    # Production workflow names do not grant permission to delegate that
    # workflow to an external package/build/runtime CLI. A test/bench path
    # was already classified above; all other matching tool spawns block.
    # (Tên workflow production không cấp quyền giao việc cho CLI ngoài;
    # test/bench đã được phân loại riêng phía trên.)
    for op in ECOSYSTEM_OWNED_OPS:
        if op in fn_parts or fn_lower.startswith(op):
            return op, "violation"

    # Diagnostic-only tool probes are non-mutating and may be reported as
    # such; this is not a native capability verdict.
    # (Probe doctor không mutation; đây không phải verdict native.)
    for op in ALLOWED_OPS:
        if op in fn_parts or fn_lower.startswith(op):
            return op, "allowed"

    # Unclassified spawn is a blocker; a marker or directory never grants
    # an exception.
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
    except OSError as error:
        return [{
            "file": rel_path,
            "line": 1,
            "tool": "<unreadable-rust>",
            "evidence_kind": "scan_error",
            "op_class": "unscanned",
            "function": "<scan-error>",
            "status": "violation",
            "declared_delegation": False,
            "scan_error": str(error),
        }]
    lines = text.splitlines()
    spans = _functions(lines)
    message_macro_spans = _message_spans(text)
    const_array_spans = _const_array_spans(lines)
    findings = []

    all_patterns = SPAWN_PATTERNS + [
        (pattern, True) for pattern in DYNAMIC_SPAWN_PATTERNS
    ]
    for pattern, matches_inside_message in all_patterns:
        for match in pattern.finditer(text):
            if not matches_inside_message and _inside_spans(
                message_macro_spans, match.start(1)
            ):
                continue
            # Report/deduplicate at the executable literal, not the start
            # of a multi-line wrapper call (which may be on another line).
            line_no = text.count("\n", 0, match.start(1)) + 1
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
            if tool in NON_EXECUTABLE_SENTINELS:
                continue
            if (
                pattern in DESCRIPTOR_PATTERNS
                and tool.split(None, 1)[0].upper() in NON_EXECUTABLE_SQL_PREFIXES
            ):
                # SQL APIs commonly look like ("statement", []) and must
                # not become process-command descriptors.
                # (SQL API thường có dạng ("câu lệnh", []), không phải
                # descriptor để chạy process.)
                continue
            if pattern in DYNAMIC_SPAWN_PATTERNS:
                tool = f"<dynamic:{tool}>"
                evidence_kind = "dynamic_spawn_call"
            elif pattern in DESCRIPTOR_PATTERNS:
                evidence_kind = "command_descriptor"
            else:
                evidence_kind = "spawn_or_wrapper_call"
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
            if span is not None and _is_test_function(lines, span[1]):
                op_class, status = "test-fixture", "allowed"
                classification_reason = None
            else:
                classification_reason = PLATFORM_PROCESS_BOUNDARY_REVIEWS.get(
                    (rel_path.replace(os.sep, "/"), fn_name, tool)
                )
                if classification_reason is not None:
                    op_class, status = "platform-process-boundary", "review-required"
                else:
                    classification_reason = AUDITED_EXECUTOR_ROUTES.get(
                        (rel_path.replace(os.sep, "/"), fn_name, tool)
                    )
                    if classification_reason is not None:
                        op_class, status = "audited-executor-route", "allowed"
                    else:
                        op_class, status = _classify(rel_path, fn_name)
            # A literal command descriptor proves a route can select an
            # executable, not that this source line spawns it. Keep unknown
            # production routes release-blocking until linked to a launcher,
            # but do not report them as observed process executions.
            if evidence_kind == "command_descriptor" and status == "violation":
                status = "review-required"
            findings.append({
                "file": rel_path,
                "line": line_no,
                "tool": tool,
                "evidence_kind": evidence_kind,
                "op_class": op_class,
                "function": fn_name,
                "status": status,
                "blocking": status in {"violation", "review-required"},
                "classification_reason": classification_reason,
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

        if not os.path.isdir(base):
            checked += 1
            missing.append(f"<scan-error:missing-root> {os.path.normpath(root)}")
            continue

        def record_walk_error(error):
            nonlocal checked
            checked += 1
            error_path = getattr(error, "filename", None) or base
            missing.append(
                f"<scan-error:walk> {os.path.relpath(error_path, repo_root)}"
            )

        for dirpath, _dirnames, filenames in os.walk(base, onerror=record_walk_error):
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
                    checked += 1
                    missing.append(f"<scan-error:unreadable> {rel_path}")
                    continue
                if "dep_gate::" in text or "GATE-EXEMPT:" in text:
                    checked += 1
                    continue
                if not any(FN_RE.match(line) for line in text.splitlines()):
                    continue  # pure router — no logic to gate
                checked += 1
                missing.append(rel_path)
    return checked, sorted(set(missing))


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    findings = []
    for abs_path in _iter_rust_files():
        rel_path = os.path.relpath(abs_path, repo_root)
        findings.extend(scan_file(rel_path, abs_path))
    python_files = _iter_python_files()
    python_process_calls = [
        item
        for abs_path in python_files
        for item in scan_python_file(abs_path, repo_root)
    ]
    shell_files = _iter_shell_files()
    shell_process_calls = [
        item
        for abs_path in shell_files
        for item in scan_shell_file(abs_path, repo_root)
    ]
    javascript_files = _iter_files_with_suffixes((".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx"))
    javascript_process_calls = [
        item
        for abs_path in javascript_files
        for item in scan_supplemental_file(abs_path, repo_root, scan_javascript_text)
    ]
    powershell_files = _iter_files_with_suffixes((".ps1", ".psm1", ".psd1"))
    powershell_process_calls = [
        item
        for abs_path in powershell_files
        for item in scan_supplemental_file(abs_path, repo_root, scan_powershell_text)
    ]

    counts = {"allowed": 0, "violation": 0, "review-required": 0}
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

    try:
        status = subprocess.run(
            ["git", "status", "--porcelain", "--untracked-files=all"],
            cwd=repo_root, capture_output=True, text=True, check=True,
        )
        dirty_paths = [line for line in status.stdout.splitlines() if line.strip()]
        working_tree_clean = not dirty_paths
    except (OSError, subprocess.CalledProcessError):
        dirty_paths = []
        working_tree_clean = None

    report = {
        "schema": "dependency-delegation-audit/10",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ),
        "commit": commit,
        "working_tree_clean": working_tree_clean,
        "dirty_paths_count": (
            len(dirty_paths) if working_tree_clean is not None else None
        ),
        "scan_roots": SCAN_ROOTS,
        "python_scan_roots": PYTHON_SCAN_ROOTS,
        "python_files_scanned": len(python_files),
        "shell_scan_roots": SHELL_SCAN_ROOTS,
        "shell_files_scanned": len(shell_files),
        "javascript_files_scanned": len(javascript_files),
        "powershell_files_scanned": len(powershell_files),
        "scan_languages": [
            "Rust process boundaries",
            "Python stdlib process APIs",
            "Shell command/process surfaces (static inventory)",
            "JavaScript/TypeScript process APIs (lexical inventory)",
            "PowerShell process APIs (lexical inventory)",
        ],
        "coverage_complete": not UNSCANNED_PROCESS_SURFACES,
        "unscanned_process_surfaces": UNSCANNED_PROCESS_SURFACES,
        "python_process_calls": python_process_calls,
        "shell_process_calls": shell_process_calls,
        "javascript_process_calls": javascript_process_calls,
        "powershell_process_calls": powershell_process_calls,
        "tools": TOOLS,
        "allowed_ops": list(ALLOWED_OPS),
        "declaration_marker": MARKER,
        "summary": {
            "total": len(findings),
            "allowed": counts["allowed"],
            "violation": counts["violation"],
            "review_required": counts["review-required"],
            "blocking": counts["violation"] + counts["review-required"],
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
        f"violation={counts['violation']} "
        f"python-process-api={len(python_process_calls)} "
        f"shell-process-surfaces={len(shell_process_calls)} "
        f"js-process-api={len(javascript_process_calls)} "
        f"powershell-process-api={len(powershell_process_calls)}"
    )
    blocked = [f for f in findings if f.get("blocking", f["status"] == "violation")]
    if blocked:
        print(
            "PLATFORM OWNERSHIP BLOCKED — resolve production process spawns and "
            "unreviewed command routes; `DELEGATED:` does not waive them:",
            file=sys.stderr,
        )
        for finding in blocked:
            declaration = " [documented debt]" if finding["declared_delegation"] else ""
            label = finding["status"]
            print(
                f"  [{label}] {finding['file']}:{finding['line']} "
                f"{finding['tool']} ({finding['function']}){declaration}",
                file=sys.stderr,
            )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    if lane_missing:
        print(
            "LANE OWNERSHIP GATE INCOMPLETE — fix scan errors or add "
            "`dep_gate::gate()` / a reasoned `GATE-EXEMPT:`:",
            file=sys.stderr,
        )
        for rel_path in lane_missing:
            label = "coverage-error" if rel_path.startswith("<scan-error:") else "no-gate"
            print(f"  [{label}] {rel_path}", file=sys.stderr)
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    if python_process_calls:
        print(
            "PYTHON PROCESS REVIEW REQUIRED — every Python process API is "
            "listed as unreviewed in the ledger:",
            file=sys.stderr,
        )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    if shell_process_calls:
        print(
            "SHELL PROCESS REVIEW REQUIRED — direct known-tool command heads "
            "are listed as unreviewed in the ledger:",
            file=sys.stderr,
        )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    if javascript_process_calls or powershell_process_calls:
        print(
            "JAVASCRIPT/POWERSHELL PROCESS REVIEW REQUIRED — lexical findings "
            "are unreviewed and block a clean result:",
            file=sys.stderr,
        )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    print(
        f"lane-gate coverage: {lane_checked} lane file(s) reference "
        "dep_gate:: or GATE-EXEMPT:"
    )
    if UNSCANNED_PROCESS_SURFACES:
        print(
            "PROCESS AUDIT INCOMPLETE — non-Rust executable and workflow command/provenance "
            "surfaces remain incomplete; a clean Rust ledger is not repository-wide evidence:",
            file=sys.stderr,
        )
        for surface in UNSCANNED_PROCESS_SURFACES:
            print(f"  [unscanned] {surface['path']}: {surface['reason']}", file=sys.stderr)
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    print(f"ledger: {os.path.relpath(out_path, repo_root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
