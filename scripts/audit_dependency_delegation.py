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
blocker, unreviewed Python/shell process call, or JavaScript/PowerShell process
finding remains. Known coverage gaps stay in the ledger and keep
`coverage_complete=false`, but do not change the direct-boundary verdict; this
gate is not a repository-complete release proof.

Heuristics (documented, deliberately conservative): Rust uses line/context
matching and a naive brace walk; Python uses AST for selected standard-library
process APIs. Neither is a complete language parser/dataflow analysis. Spawn
shapes include exec_tool(), mgc_exec run wrappers, Command::new(), which(),
tool descriptors and dynamic process APIs; unresolved patterns remain blockers.

Cổng ownership native: marker `DELEGATED:` chỉ ghi nhận debt, không miễn
trừ. Finding Rust chưa phân loại, Python/shell chưa review, hoặc process
JavaScript/PowerShell đã phát hiện sẽ làm gate đỏ. Khoảng trống coverage vẫn
được ghi trong ledger và giữ `coverage_complete=false`, nhưng không được hiểu
thành bằng chứng đã audit toàn repository.
"""

import ast
import datetime
import hashlib
import json
import os
import posixpath
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

# Known blind spots remain explicit in the ledger. They keep
# `coverage_complete=false`, but do not override a clean direct-boundary gate.
# (Điểm mù vẫn được ghi trong ledger và giữ `coverage_complete=false`; chúng
# không đổi kết quả gate khi mọi ranh giới trực tiếp đã được rà.)
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
        "path": "shell interpreter startup environment",
        "reason": "BASH_ENV and similar hooks may run before an entrypoint script starts; the invoking runner must sanitize startup variables before launching Bash, while this pass checks only child launches visible inside scanned scripts",
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

# Exact AST fingerprints for Python process calls reviewed as repository
# tooling. A changed call expression, extra duplicate, or missing reviewed
# call invalidates its entry and returns the route to review-required.
# Fingerprints bind the review to the argv construction and subprocess options,
# while the reason records why that exact process boundary is expected.
PYTHON_PROCESS_REVIEWS = {
    "benchmark/scripts/bench_v2.py": {
        ("subprocess.run", "eb078c219148886121d02e62b5582b723ac676d5b21cf0773def7b8b10f6e952"): (
            1, "Local benchmark helper; callers pass fixed benchmark/system-info argv lists.",
        ),
        ("subprocess.run", "9825f868be2838f0ee5d54a7dad9376510864232f086fb81125c69fe6b7f9316"): (
            1, "Runs install-only benchmark argv from the fixed PMS table in an isolated fixture directory.",
        ),
    },
    "scripts/audit_capability_matrix.py": {
        ("subprocess.run", "2599a48485297d50a52db1a693a0d4fb0d2c5973b52463b14dd1ac98e3da3db1"): (
            1, "Reads the current repository HEAD using fixed git arguments.",
        ),
        ("subprocess.run", "0ac2cda94516ef5622fde0b140fcef302589f1bf901fd8594d7a1cc9cae15abc"): (
            1, "Reads repository dirty state using fixed git arguments.",
        ),
        ("subprocess.run", "a11d00e9c720f21b59dded08fa29501260f450b2a90717cad7a65fc3a167e8a2"): (
            1, "Runs only the two fixed audit E2E cargo test suites with a configured timeout.",
        ),
    },
    "scripts/audit_dependency_delegation.py": {
        ("subprocess.run", "09480a7bb71a9ff8fa5c6c32b1b09f4c7949d20fdf4bcebaaaf48ef535545a23"): (
            1, "Reads repository HEAD for the generated audit ledger using fixed git arguments.",
        ),
        ("subprocess.run", "5e23c44f5fcf447106ad3d48651774d3cafdb40e679347ce60079aa9348995c3"): (
            1, "Reads repository dirty state for the generated audit ledger using fixed git arguments.",
        ),
    },
    "scripts/lifecycle_capability_matrix.py": {
        ("subprocess.run", "7cfeb3cb2645367ecc7f91a954cb0666b366f378a78a572e6fcd9bda09c2f298"): (
            1, "Shared lifecycle evidence runner; call sites execute bounded, source-defined test/build commands.",
        ),
        ("subprocess.Popen", "7b86bfefe6e49f3ae4cbd67739e790506858031072fc183fba63766f68b48bf8"): (
            1, "Starts the local mgc binary for a recovery fault-injection fixture and captures its output.",
        ),
        ("subprocess.Popen", "6794237e578f851bba22f3ea59e7c1619dd0c8013b22653fede7bda3ba918e9b"): (
            1, "Starts the local mgc binary with a source-defined lifecycle probe argv in a sandbox project.",
        ),
        ("subprocess.Popen", "a53a245f97a247c95efef054c891d2ed388e947173ba179567a90f518a662f98"): (
            1, "Starts the local mgc dev server for a bounded lifecycle readiness probe.",
        ),
    },
    "scripts/provenance_chain.py": {
        ("subprocess.run", "da765a20acc92310eb3d7ea67493e8d4e06607601d66e345801501bb18ce591c"): (
            1, "Git metadata helper; all current call sites use fixed read-only git subcommands.",
        ),
        ("subprocess.run", "7ca4d382a4b5701bc9ea0a0de05ce02e8f970dc013999128273d281d6ee295b5"): (
            1, "Checks working-tree state using fixed read-only git arguments.",
        ),
    },
    "scripts/test_audit_capability_matrix.py": {
        ("subprocess.run", "2727cd5100e6142749b140d7708f37a2c43bfbc49724efd9118dba1b5def4335"): (
            1, "Creates an isolated temporary Git repository for a unit test.",
        ),
        ("subprocess.run", "772c78f2dee4edc79f10f711d8b8eee3e46801c5ecac0e4a1e2d7ae0d9eaeb8e"): (
            1, "Sets a test-only email identity in the isolated temporary Git repository.",
        ),
        ("subprocess.run", "912af031dd06e11d70e2c37947a177fd93917015f44391b49208d11ade5db83d"): (
            1, "Sets a test-only name identity in the isolated temporary Git repository.",
        ),
        ("subprocess.run", "299937726126c3045b95c26f05e4f3b80e5bac79057572a6643a218c0b7c96de"): (
            1, "Stages only the test fixture file in the isolated temporary Git repository.",
        ),
        ("subprocess.run", "f8b434d00cbcd4282b6a7956c86ebec6c7bacf628a54a46a3039550e09514c45"): (
            1, "Creates the base commit in the isolated temporary Git repository.",
        ),
    },
    "scripts/test_github_action_pin.py": {
        ("subprocess.run", "bdbce93df67d6c33fc876bf6de34d93b23df8ccd025e294832a6cd1774340d4c"): (
            1, "Runs a test-only bash verifier with positional arguments and a fake git on the test PATH.",
        ),
    },
    "scripts/test_inventory.py": {
        ("subprocess.run", "8dceba98656b0f0b839e2fd1b3b0788e5f94ad93029626a1bc999942cbc4bf7c"): (
            2, "Reads HEAD for local test inventory metadata using fixed git arguments.",
        ),
        ("subprocess.run", "c3630e647f65713d17db619174b4411964beeecbf7e0eb2c85997a63986fc725"): (
            1, "Runs the fixed default cargo test inventory command and captures test results.",
        ),
    },
    "tools/mgc-mcp/mcp_server.py": {
        ("subprocess.run", "bdbb0096bdaa5a5ad909cc82acc2bbb7a1d7dada88c40ad6715240d8f15802e0"): (
            1, "MCP dispatch passes argv without a shell; subcommands are selected from ARGS_FOR_TOOL and timeout is fixed.",
        ),
    },
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
    "govulncheck", "composer", "pub", "npx", "bunx", "uvx", "pipx",
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
    # Core-owned compiler and runtime routes use mgc-exec's immutable tool
    # allowlist; package managers remain denied and build resolvers are offline.
    # (Compiler/runtime do MGC sở hữu đi qua allowlist mgc-exec; PM bị cấm và build chạy offline.)
    ("cli/src/commands/build.rs", "build_ai", "go"):         "mgc-exec BuildRunner; Go build uses -mod=readonly and offline environment",
    ("cli/src/commands/build.rs", "build_lib", "go"):         "mgc-exec BuildRunner; Go build uses -mod=readonly and offline environment",
    ("cli/src/commands/build.rs", "build_app", "gradle"):         "mgc-exec BuildRunner; Gradle build is --offline",
    ("cli/src/commands/build.rs", "build_app", "swift"):         "mgc-exec BuildRunner; Swift build disables package resolution",
    ("cli/src/commands/build.rs", "build_app", "flutter"):         "mgc-exec BuildRunner; Flutter build uses --no-pub",
    ("cli/src/commands/build.rs", "build_multi_app", "swift"):         "mgc-exec BuildRunner; Swift build disables package resolution",
    (
        "cli/src/commands/build/web_engine.rs",
        "run_framework_build_if_supported",
        "<dynamic:program>",
    ): "script parser + framework allowlist + BuildRunner deny package managers",
    (
        "cli/src/commands/build/web_engine.rs",
        "run_allowlisted_tool_with_env",
        "<dynamic:program>",
    ): "BuildRunner allowlist + dependency-resolution guard in mgc-exec",
    (
        "cli/src/commands/core/dev/app.rs",
        "run_tool_with_env",
        "<dynamic:cmd>",
    ): "closed app toolchain set + DevServer scope + mgc-exec package-manager deny",
    ("cli/src/commands/core/dev/app.rs", "flutter_dev_command", "flutter"):         "static Flutter device command; --no-pub and mgc-exec DevServer boundary",
    ("cli/src/commands/core/dev/app.rs", "dev_ios", "swift"):         "static Swift run command disables package resolution through mgc-exec",
    (
        "cli/src/commands/core/dev/iot.rs",
        "run_tool",
        "<dynamic:cmd>",
    ): "closed cargo/espflash selector with BuildRunner or DeviceControl scope",
    ("cli/src/commands/core/shared.rs", "ai_dev", "<dynamic:cmd>"):         "python_cmd selector + DevServer scope + mgc-exec package-manager deny",
    (
        "cli/src/commands/core/web.rs",
        "run_dev_launch_with_guard",
        "<dynamic:launch>",
    ): "validated framework launch + DevServer scope + package-manager script rejection",
    ("cli/src/commands/dev.rs", "run", "<dynamic:cmd>"):         "core-specific game/IoT command builders + DevServer/DeviceControl mgc-exec scopes",
    ("cli/src/commands/exec.rs", "run", "<dynamic:command>"):         "explicit user-selected executable passes mgc-exec allowlist and dependency-resolution guards",
    ("cli/src/commands/run.rs", "execute_task_with_bin", "<dynamic:program>"):         "parsed project task + package-manager script rejection + mgc-exec allowlist",
    ("cli/src/commands/test.rs", "test_at", "<dynamic:runner>"):         "detected test runner + TestRunner scope + mgc-exec allowlist",
    ("core/crates/mgc-config/src/hooks.rs", "run_hooks", "<dynamic:program>"):         "parsed argv + dependency lifecycle hooks denied + TestRunner scope",
    ("core/crates/mgc-exec/src/run.rs", "run_inherited", "<dynamic:cmd>"):         "central executor checks scope allowlist, forbidden package managers, and script policy before spawn",
}

# Direct `Command::new` constructors are not execution delegation. Only the
# executor's private spawn primitive and its fixed Windows batch interpreter
# receive an exact source/function/target route here. Docker's async stdin
# handoff also has one dedicated route with a closed executable and argv.
# Constructor `Command::new` không tự chứng minh delegation; chỉ cho phép
# primitive nội bộ, batch interpreter và Docker async handoff theo route hẹp.
AUDITED_DIRECT_PROCESS_ROUTES = {
    (
        "core/crates/mgc-exec/src/run.rs",
        "execute_command",
        "<dynamic:resolved_cmd>",
    ): "private executor spawn primitive after allowlist, package-manager, script, and process-tree checks",
    (
        "core/crates/mgc-exec/src/run.rs",
        "windows_batch_command",
        "<dynamic:windows_system_tool_path>",
    ): "fixed System32 cmd.exe; batch path and argv reject cmd.exe operators before construction",
    (
        "cli/src/commands/publish.rs",
        "docker_command",
        "docker",
    ): "fixed Docker CLI; argv only, isolated config, secret via stdin, async timeout, and kill-on-drop",
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

EXECUTOR_ROUTE_PATTERNS = {
    *(SPAWN_PATTERNS[index][0] for index in range(2, 6)),
    *DESCRIPTOR_PATTERNS,
    DYNAMIC_SPAWN_PATTERNS[1],
    DYNAMIC_SPAWN_PATTERNS[4],
    DYNAMIC_SPAWN_PATTERNS[5],
}
DIRECT_PROCESS_ROUTE_PATTERNS = {
    SPAWN_PATTERNS[6][0],
    DYNAMIC_SPAWN_PATTERNS[3],
}


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
SHELL_ABSOLUTE_ENV_WRAPPERS = {"/usr/bin/env", "/bin/env"}
SHELL_REDIRECTION_RE = re.compile(r"^(?:[0-9]+)?(?:&>>|&>|>>|<>|>&|<&|>|<)")
SHELL_DYNAMIC_EXECUTORS = {".", "alias", "builtin", "eval", "source", "trap"}
SHELL_INTERPRETERS = {
    "ash", "bash", "csh", "dash", "fish", "ksh", "mksh", "nu", "oksh",
    "powershell", "pwsh", "sh", "tcsh", "zsh",
}
SHELL_APPLETS = {"ash", "bash", "sh"}
SHELL_SCRIPT_SUFFIXES = (".sh", ".bash", ".zsh", ".ps1")
SHELL_TRUSTED_SYSTEM_BIN_DIRS = {"/bin", "/sbin", "/usr/bin", "/usr/sbin"}
SHELL_WRAPPER_VALUE_OPTIONS = {
    "env": {"-u", "--unset", "-C", "--chdir", "-S", "--split-string"},
    "nice": {"-n", "--adjustment"},
    "sudo": {
        "-u", "--user", "-g", "--group", "-C", "--chdir", "-h", "--host",
        "-p", "--prompt", "-R", "--chroot", "-T", "--command-timeout",
    },
    "time": {"-o", "--output", "-f", "--format"},
    "timeout": {"-k", "--kill-after", "-s", "--signal"},
}
SHELL_WRAPPER_POSITIONAL_COUNTS = {"timeout": 1}
SHELL_CONTROL_WORDS = {
    "!", "{", "}", "case", "do", "done", "elif", "else", "esac",
    "fi", "for", "if", "in", "select", "then", "until", "while",
}
SHELL_ASSIGNMENT_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(?:\[[^]]+\])?\+?=")
SHELL_ARGV_DISPATCH_REVIEWS = {
    "cli/tests/scripts/acceptance_phase2.sh": {
        'if "$@" >/dev/null 2>&1; then',
        'output=$("$@" 2>&1 || true)',
    },
    "cli/tests/scripts/all_core_scaffold_stress.sh": {
        'if "$@" >/dev/null 2>&1; then',
        'output=$("$@" 2>&1 || true)',
    },
    "cli/tests/scripts/cli_syntax_stress.sh": {
        'if "$@" >/dev/null 2>&1; then',
        'output=$("$@" 2>&1 || true)',
    },
    "scripts/test-cli.sh": {
        'output=$("$@" 2>&1)',
    },
}
SHELL_EXPLICIT_DYNAMIC_REVIEWS = {
    "scripts/smoke-test.sh": {
        'if ! version_output=$("$MGC_PATH" --version 2>&1); then',
        'if ! help_output=$("$MGC_PATH" --help 2>&1); then',
        'if version_cmd=$("$MGC_PATH" version 2>&1); then',
    },
}
SHELL_SOURCE_INSPECTION_REVIEWS = {
    "scripts/check-module-hygiene.sh": {
        'done < <(find "$dir" -name \'*.rs\' -print0 2>/dev/null | while IFS= read -r -d \'\' file; do',
    },
    "scripts/check-no-pm.sh": {
        "BAD=$(grep -rnE '\\b(npm|npx|pnpm|yarn|bun)\\b' cli/src adapters core --include='*.rs' \\",
    },
    "scripts/migrate_inline_tests.sh": {
        'files_with_inline_tests=$(grep -rl "#\\[cfg(test)\\]" adapters/ core/ cli/src/ 2>/dev/null | grep -E "\\.rs$" | grep -v "/test/" || true)',
    },
}
# Review only the exact smoke-test argv; this inventory does not certify a
# caller-selected executable's internal behavior. — Chỉ duyệt đúng argv smoke test;
# inventory này không chứng nhận hành vi bên trong executable do caller chọn.

# Dependency-manager mutations need a source-bound review, even when their
# command name is a visible literal. Hashes cover the parsed command segment, file,
# line, full source contents, and enclosing shell-control context. Any file edit,
# command move, or guard change invalidates the grant and requires a fresh review.
# (Lệnh thay đổi dependency phải có review gắn với source dù executable là literal.
# Hash gồm command, file, dòng, toàn bộ source và guard shell; mọi sửa file, di chuyển
# lệnh hoặc đổi guard đều cần review mới.)
SHELL_DEPENDENCY_COMMAND_REVIEWS = {
    'benchmark/scripts/quick_bench.sh': {
        "201462633aef5d3d5edc666da938c623da554fafe4c627dc9a3df55fd879aeb0",
        "abe607fc46bad7a650f07ffacb5bbe020e5ec8dc90676049574c49dff1b81715",
    },
    'benchmark/scripts/run_benchmark.sh': {
        "8a58d7f2235ca1cddb66667555ccc1fa7f1d5bfadc3c4fe9e40735a7f24af542",
        "2dd889b391bd7760a9150d2bc7f45ca5416ac318ddd389627f99e2232183528a",
        "9df0d55af68a92788722ec0afc412533f05d44f89b31bc26e4e7739e3c6512c2",
        "67ef6755e580ae6f152bc677d3e5a9626766a33a1e88845126144470fddd3bca",
    },
    'benchmark/scripts/run_benchmark_native.sh': {
        "c8c1e5a5cb4ea084107ee7c994621b9bdc13f2d4461296d54b32da76003439b8",
        "b97b60b2a16117468f90fcb92627935712bde057c34d0e0aff3ef9bd22791759",
        "bc21860550a152c91521aea5323b4509ed588fafe1ef5ca9a6e9859d278b552b",
        "2a1b1571c3bbea8a6bfab13476f7acd5302635a1d137ef0dfe218ad3740c08ad",
        "9344bc70be03b740441912a1b173befb008d9779a140c8a04e63f6bec117c0a4",
        "6b2ef7a7ac046b5c66ebca1c810b6e313cddc33e12b2e7d70de4c657da769559",
        "3a335ff8668e5325df15a35f49df500c06b3f947917f7ca13e7785ad9443c654",
        "ee9bf2627ded10df9d265e8f7caa6671078f3dfbb8737f534ee7ecdf7d727c42",
    },
    'benchmark/scripts/run_benchmark_phased.sh': {
        "8eea46f82aaed545925b7bac1e4e54ccb3a3e4f04fdcbdfce9bb809f64b4e82e",
        "8cfe1f0d81d033526dafa382d9f3d0ea3d9e1d4ec61116ff661faa3a4ef48b3d",
        "d74e9836ab1db51c0a545019fa480fb6ff799e912818df9bdffae016129d7449",
        "370219f80b9639a14ca668dd4f3348fe2dac43506e9c3873c3028107d415a787",
        "cafdd833df51b4b3e82ae1b74847d379a615a261853c8a66369a3f62287dbb47",
        "c90409c5675bf01f11ceb819d023acd241d81143bf626ce18d57858a7f72d4b9",
        "088bb8e469a7e7743ff21353cb930eba52598f17aff8d30c5b23933b5dba976a",
        "94ab6beb2eb340b6e48066ad6891dde8c93eb9cbc69f3e3b403de2cf19afec61",
    },
    'scripts/bench-ci.sh': {
        "ae14ff47bf2d8510c9b31ecc367454bc63d7307692ed4c390199545bb4cecb7c",
    },
    'scripts/bench.sh': {
        "fa63073d82c5485e222d2b436433a8591e074bddbb3c8dffa64dc5e540402aea",
        "fc0833bce56a3f4ed16a3c4149287cf6062227f3399c0789306d9c1ffdc57534",
        "d30628df28d92f2d4db1805cfd18df0b9cb23d37d14c321707eea5e8777abaa9",
        "c1e734ae4538b5070af94abd269b53518ffd57feee95b161c8f881c48e79320f",
        "c90459835e2ca1b58772b32e5a44c6875c3cda352867cca7068b5e27937b3d70",
        "63a1ededee0e16c504eb2de591bcd1b57300aea998b29cedcf015de8947d17c5",
        "5f98527bdd2a6a5804e95e874e1a216078597bfaedc378ba31041d9b5563eee8",
        "291e2847f4d0c8bc71cbf4bf8510557c035a2ebbd6f44764f8d430b2f325d5b9",
        "50eded51c060dcd70a1e0bf9a4e8d89a8c7b54cf49cd570a3428458556efcc72",
        "b384b1b1245c193dfb730075a7c67fdff299a52e1c7efb1175e354d2be153465",
    },
    'scripts/build.sh': {
        "a5184092f6314a2414644a9cae6b6f031626448804c2111f91c2fdc9d581fe7c",
        "7b902a6dca9b5e4c72a457e4442a49162b523c7b86d12f863764034caa7ee29c",
        "a890c9b6e122c2a1eea74d3d271e31e8b49e38cd79ee311ba2b936c3821fb1e0",
    },
    'scripts/install.sh': {
        "c48b21609d842a372096e81333bbcacc21b4f4bc54d7a173490d6d6cf321521e",
    },
    'scripts/release.sh': {
        "1e51177d8f8372894f8fd4a9e581a60a26b3f03778eabeeb06b37357aca006ee",
    },
    'cli/tests/scripts/competitive_benchmark_impl.sh': {
        "629ce7020efb842bd715a9ab592f2203ee4755659dde73f8e26c1d9ca6e8a005",
    },
    'scripts/gen-real-lockfile-fixtures.sh': {
        "87cfb57db5ce73b528ab6c8a73e94cc5d19e429e29f082a31dd2b3fc53d5af1d",
    },
    'scripts/publish.sh': {
        "aed22d7245b1f5d808558dd46791d5cd374616a2a59dcba708491199619b98f5",
        "de7828bae3eb8b9e31eb0e37ee2372d3690be736d59fff259220b2c832a69e29",
    },
}

SHELL_DEPENDENCY_MANAGER_ACTIONS = {
    "bun": {"install", "add", "remove", "uninstall", "update", "upgrade", "link", "publish", "x"},
    "cargo": {"fetch", "install", "add", "remove", "update", "publish", "run"},
    "composer": {"install", "update", "require", "remove", "create-project"},
    "dart": {"pub get", "pub upgrade", "pub add", "pub remove"},
    "deno": {"install", "add", "remove", "upgrade", "run", "task"},
    "dotnet": {"restore", "add", "remove"},
    "flutter": {"pub get", "pub upgrade", "pub add", "pub remove"},
    "go": {"get", "install", "mod download", "mod tidy", "mod vendor", "run"},
    "gradle": {"dependencies", "install", "publish"},
    "mvn": {"dependency:resolve", "dependency:get", "install", "deploy"},
    "npm": {"install", "i", "ci", "add", "remove", "uninstall", "update", "upgrade", "dedupe", "link", "publish", "pack", "cache clean", "exec"},
    "pip": {"install", "uninstall", "download"},
    "pip3": {"install", "uninstall", "download"},
    "pio": {"pkg install", "pkg update"},
    "pnpm": {"install", "i", "add", "remove", "uninstall", "update", "upgrade", "link", "publish", "pack", "store prune", "dlx"},
    "pod": {"install", "update", "repo update"},
    "pub": {"get", "upgrade", "add", "remove"},
    "swift": {"package resolve", "package update", "package add", "package remove"},
    "uv": {"install", "sync", "add", "remove", "pip install", "pip uninstall", "run", "tool run"},
    "west": {"update"},
    "yarn": {"install", "add", "remove", "upgrade", "link", "publish", "pack", "dlx"},
}


def _shell_command_segments(text: str, _line_offset: int = 0):
    """Yield logical shell command segments with their first physical line.
    (Tách command shell theo control operator, giữ dòng bắt đầu để audit.)"""
    pending = ""
    start_line = 1
    pending_continuation = False
    heredoc_delimiters = []
    case_states = []

    def split_tokens(tokens, line_number):
        segment = []
        in_double_bracket_test = False
        arithmetic_depth = 0
        for token in tokens:
            lowered = token.lower()
            if lowered == "case":
                case_states.append("selector")
                segment.append(token)
                continue
            if case_states and case_states[-1] == "selector":
                segment.append(token)
                if lowered == "in":
                    case_states[-1] = "pattern"
                    # A case selector is syntax, not an executable head.
                    # (Selector của case là cú pháp, không phải command.)
                    segment = []
                continue
            if case_states and case_states[-1] == "pattern":
                if lowered == "esac":
                    case_states.pop()
                    segment = []
                elif ")" in token:
                    case_states[-1] = "body"
                    segment = []
                continue
            if lowered == "esac" and case_states:
                if segment:
                    yield line_number, segment, None
                case_states.pop()
                segment = []
                continue
            if lowered == "case":
                case_states.append("selector")
                segment.append(token)
                continue
            if case_states and case_states[-1] == "body" and token in {";;", ";&", ";;&"}:
                if segment:
                    yield line_number, segment, None
                    segment = []
                case_states[-1] = "pattern"
                continue
            if token == "[[":
                in_double_bracket_test = True
                segment.append(token)
            elif token == "]]":
                in_double_bracket_test = False
                segment.append(token)
            elif arithmetic_depth or token.startswith("(("):
                arithmetic_depth += token.count("((") - token.count("))")
                segment.append(token)
            elif token and all(char in ";&|" for char in token) and not in_double_bracket_test:
                if segment:
                    yield line_number, segment, None
                    segment = []
            else:
                segment.append(token)
        if segment:
            yield line_number, segment, None

    heredoc_content = []
    heredoc_content_start_line = None
    for local_line_no, physical in enumerate(text.splitlines(), start=1):
        line_no = local_line_no + _line_offset
        if heredoc_delimiters:
            delimiter, strip_tabs, expandable = heredoc_delimiters[0]
            candidate = physical.lstrip("\t") if strip_tabs else physical
            if candidate.rstrip("\r\n") == delimiter:
                if expandable and heredoc_content:
                    body = "\n".join(heredoc_content)
                    for nested_source in _shell_execution_substitution_bodies(
                        body,
                        include_process_substitution=False,
                        recognize_comments=False,
                    ):
                        yield from _shell_command_segments(
                            nested_source,
                            (heredoc_content_start_line or line_no) - 1,
                        )
                heredoc_delimiters.pop(0)
                heredoc_content = []
                heredoc_content_start_line = None
            else:
                if heredoc_content_start_line is None:
                    heredoc_content_start_line = line_no
                heredoc_content.append(physical)
            continue
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
        yield from split_tokens(tokens, start_line)
        for nested_source in _shell_execution_substitution_bodies(pending):
            yield from _shell_command_segments(nested_source, start_line - 1)
        for strip_tabs, escaped_delimiter, quote, delimiter in re.findall(
            r"(?<!<)<<(?!<)(-)?\s*(?:\\([^\s;]+)|(['\"]?)([^\s'\";]+)\3)",
            pending,
        ):
            heredoc_delimiters.append((
                escaped_delimiter or delimiter,
                bool(strip_tabs),
                not escaped_delimiter and not quote,
            ))
        pending = ""
        pending_continuation = False
    if heredoc_delimiters:
        yield line_no if text.splitlines() else 1, None, "unterminated here-document"
        return
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
        yield from split_tokens(tokens, start_line)
        for nested_source in _shell_execution_substitution_bodies(pending):
            yield from _shell_command_segments(nested_source, start_line - 1)


def _shell_command_aliases(text: str) -> dict:
    """Resolve shell executable variables only from fixed source assignments.
    Chỉ phân giải biến executable dựa trên assignment cố định trong source.
    """
    assignments = {}
    for name, value in _shell_assignment_items(text):
        assignments.setdefault(name, []).append(value)

    aliases = {}
    for name in ("MGC", "MGC_BIN", "MGC_BINARY", "MGC_PATH"):
        values = assignments.get(name, [])
        if values and all(
            value.strip().strip("\"'") == "mgc"
            or
            re.search(r"(?:^|[/\\])mgc(?:$|[\s\"'}/])", value)
            or re.search(r"\b(?:command\s+-v|which)\s+mgc\b", value)
            for value in values
        ):
            aliases[name] = "mgc"
    if re.search(
        r'\[\[\s+-f\s+"\$MGC_BIN"\s+&&\s+-x\s+"\$MGC_BIN"\s+\]\]',
        text,
    ) or re.search(r'\[\s+-x\s+"\$MGC_BIN"\s+\]', text):
        # The environment-selected binary is executable only after an exact
        # regular-file and executable-bit guard in this script.
        # (Binary từ môi trường chỉ chạy sau guard chính xác file + quyền chạy.)
        aliases.setdefault("MGC_BIN", "mgc")
    if re.search(r'\[\s+-x\s+"\$MGC_BINARY"\s+\]', text) or re.search(
        r'if\s+\[\s+!\s+-x\s+"\$MGC_BINARY"\s+\]\s*;?\s*then[\s\S]{0,240}\bexit\s+1',
        text,
    ):
        # The binary override is restricted to an executable path before use.
        # (Binary override chỉ được dùng khi đường dẫn có quyền thực thi.)
        aliases.setdefault("MGC_BINARY", "mgc")

    pnpm_commands = assignments.get("PNPM_BIN", [])
    if pnpm_commands and all(
        re.fullmatch(
            r"\s*\$\(command\s+-v\s+pnpm\)\s*",
            value.strip().strip("\"'"),
        )
        for value in pnpm_commands
    ):
        aliases["PNPM_BIN"] = "pnpm"

    hash_commands = assignments.get("HASH_CMD", [])
    if hash_commands and all(
        re.match(r"\s*[\"']?(?:sha256sum|shasum)(?:\s|[\"']|$)", value)
        for value in hash_commands
    ):
        aliases["HASH_CMD"] = "checksum-tool"

    contract_paths = assignments.get("CONTRACT", [])
    if contract_paths and all("release-artifact-contract.sh" in value for value in contract_paths):
        aliases["CONTRACT"] = "shell-script"

    for name in ("SCRIPT_DIR", "ROOT", "REPO_ROOT", "PROJECT_ROOT"):
        values = assignments.get(name, [])
        if values and all(
            "BASH_SOURCE" in value or "$0" in value or "$SCRIPT_DIR" in value
            for value in values
        ):
            aliases[name] = "shell-script-dir" if name == "SCRIPT_DIR" else "shell-script-root"

    matrix_commands = assignments.get("MATRIX_BIN", [])
    if matrix_commands and all(
        re.match(r"\s*[\"']?cargo\s+run\s+--bin\s+bench_matrix\b", value)
        for value in matrix_commands
    ):
        aliases["MATRIX_BIN"] = "cargo"

    dry_run_values = assignments.get("DRY_RUN", [])
    if dry_run_values and all(re.match(r"\s*(?:true|false)\s*$", value) for value in dry_run_values):
        aliases["DRY_RUN"] = "boolean-selector"

    pm_commands = assignments.get("PM_CMD", [])
    if pm_commands and re.search(r"case\s+\"\$PM_NAME\"\s+in", text):
        resolved = []
        for value in pm_commands:
            if "MGC_BINARY" in value and re.search(r"\binstall-web\b", value):
                resolved.append("mgc")
                continue
            match = re.match(r"\s*[\"']?(npm|pnpm|bun|yarn)\b", value)
            if not match:
                resolved = []
                break
            resolved.append(match.group(1))
        if resolved and set(resolved) <= {"mgc", "npm", "pnpm", "bun", "yarn"}:
            aliases["PM_CMD"] = "bounded-package-manager-selector"
    return aliases


def _shell_static_script_aliases(text: str, rel_path: str) -> dict:
    """Resolve only source-defined paths to scripts in this repository.
    (Chỉ phân giải đường dẫn script cố định trong source của repository.)"""
    assignments = {}
    for name, value in _shell_assignment_items(text):
        assignments.setdefault(name, []).append(value)
    source_dir = posixpath.dirname(rel_path)
    aliases = {}
    for _ in range(4):
        changed = False
        for name, values in assignments.items():
            if not values:
                continue
            resolved_values = []
            for raw_value in values:
                candidates = _shell_directory_assignment_paths(
                    raw_value, source_dir, aliases
                )
                if not candidates:
                    resolved_values = []
                    break
                resolved_values.append(candidates)
            if resolved_values:
                common = set.intersection(*resolved_values)
                if common and aliases.get(name) != common:
                    aliases[name] = common
                    changed = True
        if not changed:
            break
    file_aliases = {name: set(paths) for name, paths in aliases.items()}
    for _ in range(3):
        changed = False
        for name, values in assignments.items():
            if name in aliases or not values:
                continue
            resolved_values = []
            for raw_value in values:
                value = raw_value.strip()
                if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
                    value = value[1:-1]
                match = re.fullmatch(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?/(.*)", value)
                if not match or match.group(1) not in file_aliases:
                    resolved_values = []
                    break
                suffix = match.group(2)
                if (
                    not suffix.endswith(SHELL_SCRIPT_SUFFIXES)
                    or any(marker in suffix for marker in ("$", "`", ";", "(", ")"))
                ):
                    resolved_values = []
                    break
                candidates = {
                    posixpath.normpath(posixpath.join(base, suffix))
                    for base in file_aliases[match.group(1)]
                }
                if any(path in {"..", "."} or path.startswith("../") for path in candidates):
                    resolved_values = []
                    break
                resolved_values.append(candidates)
            if resolved_values:
                common = set.intersection(*resolved_values)
                if common:
                    file_aliases[name] = common
                    changed = True
        if not changed:
            break
    return file_aliases


def _shell_strip_comment(line: str) -> str:
    """Remove shell comments without inspecting text inside quotes.
    (Bỏ comment shell nhưng không coi `#` trong chuỗi trích dẫn là comment.)"""
    quote = None
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            escaped = False
            continue
        if char == "\\" and quote != "'":
            escaped = True
            continue
        if quote:
            if char == quote:
                quote = None
            continue
        if char in {"'", '"'}:
            quote = char
            continue
        if char == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def _shell_assignment_items(text: str):
    """Yield one-line assignments after shell-aware comment removal.
    (Trả assignment một dòng sau khi bỏ comment theo quy tắc quote shell.)"""
    pattern = re.compile(r"^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$")
    for line in text.splitlines():
        match = pattern.match(_shell_strip_comment(line))
        if match:
            yield match.group(1), match.group(2)


def _shell_alias_variable(token: str) -> str:
    value = token.strip("\"'").removeprefix("$").strip("{}")
    return value.split("[", 1)[0]


def _shell_arithmetic_writes_alias(tokens: list, alias_names: set) -> bool:
    """Detect arithmetic writes to script-path aliases.
    (Phát hiện phép toán số học ghi đè alias đường dẫn script.)"""
    source = " ".join(tokens)
    expressions = re.findall(r"(?:\$\(\(|\(\()[\s\S]*?\)\)", source)
    for expression in expressions:
        for name in alias_names:
            identifier = re.escape(name)
            prefix_or_suffix_write = re.search(
                rf"(?<![A-Za-z0-9_])(?:\+\+|--)\s*{identifier}(?![A-Za-z0-9_])"
                rf"|(?<![A-Za-z0-9_]){identifier}(?![A-Za-z0-9_])\s*(?:\+\+|--)",
                expression,
            )
            assignment_write = re.search(
                rf"(?<![A-Za-z0-9_]){identifier}(?![A-Za-z0-9_])\s*"
                r"(?:\*\*|<<|>>|[+\-*/%&|^])?=(?!=)",
                expression,
            )
            if prefix_or_suffix_write or assignment_write:
                return True
    return False


def _shell_script_alias_mutation_index(
    segments: list, aliases: dict, rel_path: str, text: str
):
    """Find the first command that can rewrite a static script-path alias.
    (Tìm command đầu tiên có thể ghi đè alias đường dẫn script tĩnh.)"""
    alias_names = set(aliases)
    if not alias_names:
        return None
    source_dir = posixpath.dirname(rel_path)
    safe_assignment_lines = set()
    for line_no, line in enumerate(text.splitlines(), start=1):
        for name, value in _shell_assignment_items(line):
            if name not in aliases:
                continue
            safe_directory = _shell_directory_assignment_paths(
                value, source_dir, aliases
            ) == aliases[name]
            normalized_value = value.strip()
            if (
                len(normalized_value) >= 2
                and normalized_value[0] == normalized_value[-1]
                and normalized_value[0] in "\"'"
            ):
                normalized_value = normalized_value[1:-1]
            safe_file = _shell_script_argument_targets(
                normalized_value, aliases
            ) == aliases[name]
            if safe_directory or safe_file:
                safe_assignment_lines.add(line_no)

    for segment_index, (line_no, segment, parse_error) in enumerate(segments):
        if parse_error:
            return segment_index
        if not segment:
            continue
        if _shell_arithmetic_writes_alias(segment, alias_names):
            return segment_index
        for token in segment:
            assignment = re.match(
                r"^([A-Za-z_][A-Za-z0-9_]*)(?:\[[^]]+\])?\+?=", token
            )
            if assignment and assignment.group(1) in alias_names:
                if line_no not in safe_assignment_lines:
                    return segment_index
            parameter_assignment = re.search(
                r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?:\[[^]]+\])?(?::?=)",
                token,
            )
            if parameter_assignment and parameter_assignment.group(1) in alias_names:
                return segment_index
        for index, token in enumerate(segment[:-1]):
            if token.lower() in {"for", "select"} and _shell_alias_variable(
                segment[index + 1]
            ) in alias_names:
                return segment_index

        command_index = 0
        while command_index < len(segment):
            token = segment[command_index]
            if token in SHELL_CONTROL_WORDS or SHELL_ASSIGNMENT_RE.match(token):
                command_index += 1
                continue
            if token in {"!", "{"}:
                command_index += 1
                continue
            break
        if command_index >= len(segment):
            continue
        command = segment[command_index].replace("\\", "/").rsplit("/", 1)[-1].lower()
        args = segment[command_index + 1 :]
        if command in {"builtin", "command"} and args:
            command = args[0].replace("\\", "/").rsplit("/", 1)[-1].lower()
            args = args[1:]
        if command in {"eval", "source", "."}:
            return segment_index
        if command == "read":
            destinations = []
            index = 0
            value_options = {"-d", "-i", "-n", "-N", "-p", "-t", "-u"}
            while index < len(args):
                arg = args[index]
                if arg == "-a":
                    if index + 1 < len(args):
                        destinations.append(_shell_alias_variable(args[index + 1]))
                    index += 2
                elif arg in value_options:
                    index += 2
                elif arg.startswith("-"):
                    index += 1
                else:
                    destinations.append(_shell_alias_variable(arg))
                    index += 1
            if not destinations:
                destinations = ["REPLY"]
            if any(name in alias_names for name in destinations) or any(
                "$" in arg
                for index, arg in enumerate(args)
                if not arg.startswith("-")
                and not (index > 0 and args[index - 1] in value_options | {"-a"})
            ):
                return segment_index
        elif command in {"mapfile", "readarray"}:
            destinations = [
                _shell_alias_variable(arg)
                for arg in args
                if not arg.startswith("-")
            ]
            if not destinations:
                destinations = ["MAPFILE"]
            if any(name in alias_names for name in destinations):
                return segment_index
        elif command == "printf":
            for index, arg in enumerate(args[:-1]):
                if arg == "-v":
                    destination = args[index + 1]
                    if _shell_alias_variable(destination) in alias_names or "$" in destination:
                        return segment_index
        elif command == "unset":
            if any(_shell_alias_variable(arg) in alias_names or "$" in arg for arg in args):
                return segment_index
        elif command in {"declare", "typeset", "local"}:
            if any(arg == "-n" or arg.startswith("-n") for arg in args):
                return segment_index
            if any(
                _shell_alias_variable(arg.partition("=")[0]) in alias_names
                for arg in args
                if not arg.startswith("-")
            ):
                return segment_index
    return None


def _shell_directory_assignment_paths(value: str, source_dir: str, aliases: dict) -> set:
    """Resolve only exact source-anchored directory expressions.
    (Chỉ phân giải biểu thức thư mục khớp chính xác và neo theo source.)"""
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        value = value[1:-1]
    source_match = re.fullmatch(
        r'\$\(\s*cd\s+"\$\(\s*dirname\s+"(?:\$\{BASH_SOURCE\[0\]\}|\$0)"\)'
        r'(?P<up>(?:/\.\.)*)"\s*&&\s*pwd\s*\)',
        value,
    )
    if source_match:
        base = source_dir
        for _ in range(source_match.group("up").count("/..")):
            base = posixpath.dirname(base)
        normalized = posixpath.normpath(base)
        return {"" if normalized == "." else normalized}

    relative_match = re.fullmatch(
        r'\$\(\s*cd\s+"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?'
        r'(?P<up>(?:/\.\.)*)"\s*&&\s*pwd\s*\)',
        value,
    )
    if not relative_match:
        relative_match = re.fullmatch(
            r'\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?(?P<up>(?:/\.\.)*)', value
        )
    if not relative_match:
        return set()
    bases = aliases.get(relative_match.group(1), set())
    paths = set()
    for base in bases:
        path = base
        for _ in range(relative_match.group("up").count("/..")):
            path = posixpath.dirname(path)
        normalized = posixpath.normpath(path)
        paths.add("" if normalized == "." else normalized)
    return paths


def _shell_script_argument_targets(argument: str, aliases: dict) -> set:
    """Resolve a shell script variable only to normalized repository paths.
    (Chỉ phân giải biến script về đường dẫn đã chuẩn hóa trong repository.)"""
    variable = re.fullmatch(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?(?:/(.*))?", argument)
    if not variable:
        return set()
    base_paths = aliases.get(variable.group(1), set())
    suffix = variable.group(2)
    if suffix is None:
        return {path for path in base_paths if path.endswith(SHELL_SCRIPT_SUFFIXES)}
    if (
        suffix.startswith("/")
        or any(marker in suffix for marker in ("$", "`", "*", "?", "[", "]"))
        or any(part == ".." for part in suffix.split("/"))
    ):
        return set()
    candidates = {
        posixpath.normpath(posixpath.join(base, suffix))
        for base in base_paths
    }
    return {
        path for path in candidates
        if path not in {"..", "."} and not path.startswith("../")
    }


def _shell_environment_unset_before(segment: list, variable: str, command_index: int) -> bool:
    """Require a clean environment before launching a Bash child.
    (Yêu cầu môi trường sạch trước khi chạy Bash con.)"""
    sanitized = False
    index = 0
    while index < command_index:
        env_token = segment[index].replace("\\", "/").lower()
        is_absolute_env = env_token in SHELL_ABSOLUTE_ENV_WRAPPERS
        is_command_env = (
            index > 0
            and segment[index - 1].lower() == "command"
            and env_token == "env"
        )
        if not (is_absolute_env or is_command_env):
            index += 1
            continue
        index += 1
        while index < command_index:
            argument = segment[index]
            if argument == "-i" or argument == "--ignore-environment":
                sanitized = True
                index += 1
                continue
            if SHELL_ASSIGNMENT_RE.match(argument):
                name = argument.partition("=")[0]
                if name == variable or re.match(r"^BASH_FUNC_[^=]*%%$", name):
                    return False
                index += 1
                continue
            if argument.startswith("-"):
                index += 1
                if argument in {"-u", "--unset"}:
                    if index >= command_index:
                        return False
                    index += 1
                    continue
                if argument.startswith("--unset="):
                    index += 1
                    continue
                if argument in SHELL_WRAPPER_VALUE_OPTIONS["env"] and index < command_index:
                    index += 1
                continue
            break
    return sanitized


def _shell_function_defined_before(text: str, name: str, line_no: int) -> bool:
    pattern = re.compile(
        r"^\s*(?:function\s+)?" + re.escape(name) + r"\s*(?:\(\s*\))?\s*\{"
    )
    lines = text.splitlines()
    for index, line in enumerate(lines[: line_no - 1]):
        if not pattern.match(line) or line.rstrip().split("{", 1)[-1].strip():
            continue
        if any(
            re.fullmatch(r"\s*}\s*;?\s*", body_line)
            for body_line in lines[index + 1 : line_no - 1]
        ):
            return True
    return False


def _shell_trap_action_is_static(text: str, line_no: int, segment: list) -> bool:
    """Accept only empty traps, prior local functions, or literal non-tool actions.
    (Chỉ chấp nhận trap rỗng, function nội bộ có trước, hoặc action literal không gọi tool.)"""
    action_index = 1
    if action_index < len(segment) and segment[action_index] == "--":
        action_index += 1
    if action_index >= len(segment) or segment[action_index] in {"", "-"}:
        return True
    action = segment[action_index]
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", action):
        return _shell_function_defined_before(text, action, line_no) and not (
            _shell_trap_callback_mutated_after(text, action, line_no)
        )
    nested_segments = list(_shell_command_segments(action))
    if not nested_segments:
        return False
    aliases = _shell_command_aliases(text)
    static_aliases = _shell_static_script_aliases(text, "trap-action.sh")
    for _, nested, parse_error in nested_segments:
        if parse_error:
            return False
        executable = _shell_executable(nested, aliases, static_aliases)
        if executable is not None:
            return False
    return True


def _shell_trap_callback_mutated_after(text: str, name: str, line_no: int) -> bool:
    """Reject a static trap review if later code can remove or replace its callback.
    (Bỏ review trap tĩnh nếu code sau đó có thể xóa hoặc thay callback.)"""
    function_header = re.compile(
        r"^\s*(?:function\s+)?" + re.escape(name) + r"\s*(?:\(\s*\))?\s*\{"
    )
    for command_line, segment, parse_error in _shell_command_segments(text):
        if command_line <= line_no or parse_error or not segment:
            continue
        visible_command = " ".join(segment)
        if not function_header.match(_shell_strip_comment(visible_command)):
            continue
        return True

    for command_line, segment, parse_error in _shell_command_segments(text):
        if command_line <= line_no or parse_error or not segment:
            continue
        command_index = 0
        while command_index < len(segment) and (
            segment[command_index] in SHELL_CONTROL_WORDS
            or SHELL_ASSIGNMENT_RE.match(segment[command_index])
        ):
            command_index += 1
        if command_index >= len(segment):
            continue
        command = segment[command_index].replace("\\", "/").rsplit("/", 1)[-1].lower()
        if command != "unset":
            continue
        arguments = segment[command_index + 1 :]
        function_unset = any(
            argument == "-f" or (argument.startswith("-") and "f" in argument[1:])
            for argument in arguments
        )
        if not function_unset:
            continue
        names = [argument for argument in arguments if not argument.startswith("-")]
        if not names or any("$" in argument or "`" in argument for argument in names):
            return True
        if any(_shell_alias_variable(argument) == name for argument in names):
            return True
    return False


def _shell_interpreter_script_target(segment: list, executable: str, aliases: dict) -> set:
    """Return statically resolved script paths, or an empty set if dynamic.
    (Trả đường dẫn script phân giải tĩnh; trả tập rỗng nếu đường dẫn động.)"""
    interpreter_index = next(
        (
            index
            for index, token in enumerate(segment)
            if token.replace("\\", "/").rsplit("/", 1)[-1].lower() == executable
        ),
        None,
    )
    if interpreter_index is None:
        return set()
    args = segment[interpreter_index + 1 :]
    if executable in {"powershell", "pwsh"}:
        for index, argument in enumerate(args):
            if argument.lower() in {"-command", "-c", "-encodedcommand", "-enc"}:
                return set()
            if argument.lower() in {"-file", "-f"}:
                if index + 1 >= len(args):
                    return set()
                args = args[index + 1 :]
                break
        else:
            return set()
    else:
        index = 0
        while index < len(args):
            argument = args[index]
            if argument in {"-c", "-s", "--command", "--stdin"} or argument.startswith("<<"):
                return set()
            if argument == "--":
                index += 1
                break
            if executable == "bash" and (
                argument == "--login"
                or re.fullmatch(r"-[A-Za-z]*l[A-Za-z]*", argument)
            ):
                return set()
            if argument in {"--rcfile", "--init-file"} or argument.startswith(
                ("--rcfile=", "--init-file=")
            ):
                # Startup hooks execute before the reviewed script. Treat them
                # as a dynamic boundary unless they receive their own scan.
                # (Hook khởi động chạy trước script đã rà; chặn nếu chưa được quét riêng.)
                return set()
            if argument in {"-o", "+o", "-O", "+O"}:
                index += 2
                continue
            if re.fullmatch(r"-[A-Za-z]+", argument) or re.fullmatch(r"\+[A-Za-z]+", argument):
                if "i" in argument:
                    return set()
                index += 1
                continue
            if argument.startswith("-"):
                return set()
            break
        args = args[index:]
    if not args:
        return set()

    argument = args[0]
    candidates = set()
    variable = re.fullmatch(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?(?:/(.*))?", argument)
    if variable:
        base_paths = aliases.get(variable.group(1), set())
        suffix = variable.group(2) or ""
        candidates.update(posixpath.normpath(posixpath.join(base, suffix)) for base in base_paths)
    return {
        path
        for path in candidates
        if path.endswith(SHELL_SCRIPT_SUFFIXES) and path != ".." and not path.startswith("../")
    }


def _shell_control_context(text: str, line_no: int) -> list:
    """Capture enclosing shell control lines for source-bound review.
    (Ghi lại guard shell đang bao quanh để review gắn với ngữ cảnh source.)"""
    stack = []
    for line in text.splitlines()[: max(0, line_no - 1)]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        head = re.match(r"(elif|else|fi|done|esac|if|for|while|until|case)\b", stripped)
        if not head:
            continue
        keyword = head.group(1)
        if keyword in {"fi", "done", "esac"}:
            if stack:
                stack.pop()
        elif keyword in {"elif", "else"}:
            if stack:
                stack[-1] = stripped
        else:
            stack.append(stripped)
    return stack


def _shell_execution_substitution_bodies(
    value: str,
    *,
    include_process_substitution: bool = True,
    recognize_comments: bool = True,
) -> list:
    """Extract executable shell substitutions from raw command text.
    (Tách command substitution đang thực thi từ shell source thô.)"""
    bodies = []
    index = 0
    quote = None
    escaped = False
    while index < len(value):
        char = value[index]
        if escaped:
            escaped = False
            index += 1
            continue
        if char == "\\" and quote != "'":
            escaped = True
            index += 1
            continue
        if quote == "'":
            if char == "'":
                quote = None
            index += 1
            continue
        if recognize_comments and quote is None and char == "#" and (
            index == 0 or value[index - 1].isspace()
        ):
            newline = value.find("\n", index)
            if newline < 0:
                break
            index = newline + 1
            continue
        if quote is None and char in {"'", '"'}:
            quote = char
            index += 1
            continue
        if quote == '"' and char == '"':
            quote = None
            index += 1
            continue
        if value.startswith("$((", index):
            # Arithmetic expansion is not a command substitution; keep scanning
            # its body so nested executable `$()` calls remain visible.
            # (Arithmetic expansion không phải command substitution; vẫn rà
            # phần thân để thấy `$()` executable lồng bên trong.)
            index += 3
            continue

        marker = None
        if value.startswith("$(", index):
            marker = "$"
        elif include_process_substitution and quote is None and value.startswith("<(", index):
            marker = "<"
        elif include_process_substitution and quote is None and value.startswith(">(", index):
            marker = ">"
        if marker is not None:
            start = index + 2
            cursor = start
            depth = 1
            nested_quote = None
            escaped = False
            while cursor < len(value):
                nested_char = value[cursor]
                if escaped:
                    escaped = False
                    cursor += 1
                    continue
                if nested_char == "\\" and nested_quote != "'":
                    escaped = True
                    cursor += 1
                    continue
                if nested_quote == "'":
                    if nested_char == "'":
                        nested_quote = None
                    cursor += 1
                    continue
                if nested_quote is None and nested_char in {"'", '"'}:
                    nested_quote = nested_char
                    cursor += 1
                    continue
                if nested_quote == '"' and nested_char == '"':
                    nested_quote = None
                    cursor += 1
                    continue
                if recognize_comments and nested_quote is None and nested_char == "#" and (
                    cursor == 0 or value[cursor - 1].isspace()
                ):
                    newline = value.find("\n", cursor)
                    if newline < 0:
                        cursor = len(value)
                        break
                    cursor = newline + 1
                    continue
                nested_prefixes = ("$(",) if nested_quote or not include_process_substitution else ("$(", "<(", ">(")
                if any(value.startswith(prefix, cursor) for prefix in nested_prefixes):
                    depth += 1
                    cursor += 2
                    continue
                if nested_quote is None and nested_char == "(":
                    depth += 1
                elif nested_quote is None and nested_char == ")":
                    depth -= 1
                    if depth == 0:
                        bodies.append(value[start:cursor])
                        index = cursor + 1
                        break
                cursor += 1
            else:
                index += len(marker) + 1
            continue
        if char == "`":
            start = index + 1
            cursor = start
            escaped = False
            while cursor < len(value):
                if escaped:
                    escaped = False
                elif value[cursor] == "\\":
                    escaped = True
                elif value[cursor] == "`":
                    bodies.append(value[start:cursor])
                    index = cursor + 1
                    break
                cursor += 1
            else:
                index += 1
            continue
        index += 1
    return bodies


def _shell_bash_shebang_is_login(shebang: str) -> bool:
    try:
        arguments = shlex.split(shebang.removeprefix("#!").strip())
    except ValueError:
        return True
    return any(
        argument in {"--login", "--interactive"}
        or re.fullmatch(r"-[A-Za-z]*[li][A-Za-z]*", argument)
        for argument in arguments[1:]
    )


def _shell_executable(
    segment: list,
    command_aliases=None,
    static_script_aliases=None,
    scanned_shell_paths=None,
):
    """Resolve a direct known-tool command after common transparent wrappers.
    (Tìm executable thuộc inventory sau wrapper shell phổ biến.)"""
    index = 0
    command_aliases = command_aliases or {}
    static_script_aliases = static_script_aliases or {}
    scanned_shell_paths = set(scanned_shell_paths or ())
    while index < len(segment):
        token = segment[index]
        lowered = token.lower()
        if lowered == "case":
            index += 1
            while index < len(segment) and segment[index].lower() != "in":
                index += 1
            index += 1
            continue
        if token == "[[":
            index += 1
            while index < len(segment) and segment[index] != "]]":
                index += 1
            index += 1
            continue
        if token.startswith("(("):
            index += 1
            while index < len(segment) and not segment[index].endswith("))"):
                index += 1
            index += 1
            continue
        if lowered in SHELL_CONTROL_WORDS:
            index += 1
            continue
        if lowered == "command" and index + 1 < len(segment) and segment[index + 1] == "-v":
            # `command -v` is a shell builtin probe, not tool execution.
            # (`command -v` là builtin dò đường dẫn, không chạy executable.)
            return None
        if token in {"(", ")", "[[", "]]"}:
            index += 1
            continue
        if SHELL_ASSIGNMENT_RE.match(token):
            # Array values are shell data, not separate command heads.
            # (Giá trị mảng là dữ liệu shell, không phải command riêng.)
            array_assignment = re.match(r"^[A-Za-z_][A-Za-z0-9_]*\+?=\(", token)
            if array_assignment:
                array_depth = token.count("(") - token.count(")")
                index += 1
                while array_depth > 0 and index < len(segment):
                    array_depth += segment[index].count("(") - segment[index].count(")")
                    index += 1
                continue
            if "$((" in token:
                arithmetic_depth = token.count("(") - token.count(")")
                index += 1
                while arithmetic_depth > 0 and index < len(segment):
                    arithmetic_depth += (
                        segment[index].count("(") - segment[index].count(")")
                    )
                    index += 1
                continue
            substitution_depth = len(re.findall(r"(?:\$|<|>)\(", token)) - token.count(")")
            index += 1
            while substitution_depth > 0 and index < len(segment):
                substitution_depth += (
                    len(re.findall(r"(?:\$|<|>)\(", segment[index]))
                    - segment[index].count(")")
                )
                index += 1
            continue
        if SHELL_REDIRECTION_RE.match(token):
            # Redirection operands are paths, not executable heads.
            # (Toán hạng redirect là đường dẫn, không phải command.)
            redirection_only = bool(
                re.fullmatch(r"(?:[0-9]+)?(?:&>>|&>|>>|<>|>&|<&|>|<)", token)
            )
            index += 1
            if redirection_only and index < len(segment):
                index += 1
            continue
        absolute_token = token.replace("\\", "/").lower()
        if lowered in SHELL_WRAPPERS or absolute_token in SHELL_ABSOLUTE_ENV_WRAPPERS:
            wrapper = "env" if absolute_token in SHELL_ABSOLUTE_ENV_WRAPPERS else lowered
            index += 1
            positional_remaining = SHELL_WRAPPER_POSITIONAL_COUNTS.get(wrapper, 0)
            while index < len(segment):
                argument = segment[index]
                if SHELL_ASSIGNMENT_RE.match(argument):
                    index += 1
                    continue
                if wrapper == "env" and re.match(r"^BASH_FUNC_[^=]*%%=", argument):
                    return "<dynamic-shell-command>"
                if argument.startswith("-"):
                    if wrapper == "env" and (
                        argument in {"-S", "--split-string"}
                        or argument.startswith(("-S", "--split-string="))
                    ):
                        # env -S reparses one argument as an executable string;
                        # the generic wrapper token walk cannot model that shell.
                        # (env -S phân tích lại chuỗi thành command; parser wrapper không mô phỏng an toàn.)
                        return "<dynamic-shell-command>"
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
        ):
            variable = re.match(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)", token)
            if variable:
                alias = command_aliases.get(variable.group(1))
                if alias in {"shell-script-dir", "shell-script-root"}:
                    targets = _shell_script_argument_targets(token, static_script_aliases)
                    if len(targets) == 1 and targets <= scanned_shell_paths:
                        return "shell-script"
                    return "<dynamic-shell-command>"
                if alias:
                    return alias
            return "<dynamic-shell-command>"
        basename = token.replace("\\", "/").rsplit("/", 1)[-1]
        if basename == "busybox" and index + 1 < len(segment):
            applet = segment[index + 1].replace("\\", "/").rsplit("/", 1)[-1]
            if applet in SHELL_APPLETS:
                return "<dynamic-shell-command>"
        if basename in SHELL_DYNAMIC_EXECUTORS:
            if basename in {"source", "."} and index + 1 < len(segment):
                source_target = segment[index + 1]
                targets = _shell_script_argument_targets(source_target, static_script_aliases)
                if len(targets) == 1 and targets <= scanned_shell_paths:
                    return "shell-source"
            return "<dynamic-shell-command>"
        if basename in SHELL_INTERPRETERS:
            # A literal interpreter plus a script path is visible and each
            # source file is scanned separately. Inline `-c` remains dynamic.
            # Interpreter + đường dẫn script rõ ràng được kiểm kê riêng; `-c`
            # nội tuyến vẫn là command động cần chặn.
            interpreter_args = segment[index + 1:]
            if (
                not interpreter_args
                or interpreter_args[0] in {"-c", "-command", "-Command"}
                or any(argument.startswith("<<") for argument in interpreter_args)
            ):
                return "<dynamic-shell-command>"
            return basename
        if basename in TOOLS:
            return basename
        if token.lower().endswith(SHELL_SCRIPT_SUFFIXES):
            # A direct script path depends on the caller's working directory;
            # only source-anchored aliases are safe to resolve statically.
            # (Đường dẫn script trực tiếp phụ thuộc working directory; chỉ alias
            # được neo theo source mới có thể phân giải tĩnh an toàn.)
            return "<dynamic-shell-command>"
        if token.startswith(("./", "../")):
            # Relative executable paths can name extensionless local scripts.
            # (Đường dẫn tương đối có thể gọi script nội bộ không có phần mở rộng.)
            return "<dynamic-shell-command>"
        if "/" in token.replace("\\", "/"):
            normalized_path = posixpath.normpath(token.replace("\\", "/"))
            parent = posixpath.dirname(normalized_path)
            if not posixpath.isabs(normalized_path) or parent not in SHELL_TRUSTED_SYSTEM_BIN_DIRS:
                # Non-system paths may be extensionless scripts or caller-controlled
                # executables, so keep them visible even without a shell suffix.
                # (Đường dẫn ngoài thư mục binary hệ thống có thể là script không
                # đuôi hoặc executable do caller kiểm soát; phải báo động.)
                return "<dynamic-shell-command>"
        break
    return None


def _shell_dependency_lifecycle_action(executable: str, segment: list, command_aliases: dict):
    """Return a dependency-mutating action reachable from this command head.
    (Trả về thao tác thay đổi dependency gắn với command head đã tìm thấy.)"""
    if executable in {"npx", "bunx", "uvx", "pipx"}:
        return "exec" if executable in {"npx", "bunx"} else "run"
    if executable == "bounded-package-manager-selector":
        managers = set(SHELL_DEPENDENCY_MANAGER_ACTIONS)
    elif executable in SHELL_DEPENDENCY_MANAGER_ACTIONS:
        managers = {executable}
    elif executable in {"python", "python3"}:
        managers = {"pip"}
    else:
        return None

    # Use tokens already parsed as shell syntax, and recognize fixed command
    # aliases resolved by _shell_command_aliases. Never treat an arbitrary
    # argument containing a manager name as the executable.
    # (Dùng token shell đã parse và alias cố định; không coi argument bất kỳ
    # có chữ tên manager là executable.)
    for index, token in enumerate(segment):
        nested_tail = None
        normalized = token.strip("\"'`()")
        variable = normalized.removeprefix("$").strip("{}")
        manager = command_aliases.get(variable)
        if manager == "bounded-package-manager-selector":
            matched_managers = managers
        elif manager in managers:
            matched_managers = {manager}
        else:
            basename = normalized.rsplit("/", 1)[-1]
            matched_managers = {basename} if basename in managers else set()
        if not matched_managers:
            # Command substitutions can be lexed as a token prefix such as
            # `$(npm`; keep that case bounded to the known manager grammar.
            nested = re.search(
                r"(?:\$\(|`)(?:[^\s;&|]*/)?([A-Za-z0-9_.-]+)", token
            )
            if nested and nested.group(1) in managers:
                matched_managers = {nested.group(1)}
                nested_tail = token[nested.end() :]
            else:
                continue

        tail = " ".join(
            ([nested_tail] if nested_tail is not None else [])
            + segment[index + 1 :]
        )
        if executable in {"python", "python3"}:
            if not re.search(r"(?:^|\s)-m\s+pip(?:3)?(?:\s|$)", tail):
                continue
            tail = re.split(r"(?:^|\s)-m\s+pip(?:3)?(?:\s|$)", tail, maxsplit=1)[-1]

        for candidate in matched_managers:
            actions = SHELL_DEPENDENCY_MANAGER_ACTIONS[candidate]
            # Matching any command verb after the executable intentionally
            # fails closed for global options with separate values (for
            # example `npm --prefix dir install`). Exact source fingerprints
            # below prevent changed or newly added invocations from inheriting
            # a review.
            # (Tìm verb sau executable để không lọt global option có value;
            # fingerprint source chính xác ngăn lệnh mới thừa kế review.)
            for action in sorted(actions, key=len, reverse=True):
                if re.search(
                    r"(?<![A-Za-z0-9_-])" + re.escape(action) + r"(?![A-Za-z0-9_-])",
                    tail,
                ):
                    return action
    return None


def scan_shell_text(rel_path: str, text: str, scanned_shell_paths=None) -> list:
    """Inventory direct known-tool shell commands; malformed input is unreviewed.
    (Kiểm kê command gọi tool đã biết; cú pháp lỗi không được xem là sạch.)"""
    findings = []
    seen_dependency_fingerprints = set()
    command_aliases = _shell_command_aliases(text)
    static_script_aliases = _shell_static_script_aliases(text, rel_path)
    command_segments = list(_shell_command_segments(text))
    alias_mutation_index = _shell_script_alias_mutation_index(
        command_segments, static_script_aliases, rel_path, text
    )
    scanned_shell_sources = (
        scanned_shell_paths if isinstance(scanned_shell_paths, dict) else {}
    )
    scanned_shell_paths = set(scanned_shell_paths or ())
    for segment_index, (line_no, segment, parse_error) in enumerate(command_segments):
        active_script_aliases = (
            static_script_aliases
            if alias_mutation_index is None or segment_index <= alias_mutation_index
            else {}
        )
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
        source_line = text.splitlines()[line_no - 1].strip()
        if (
            rel_path in SHELL_ARGV_DISPATCH_REVIEWS
            and source_line in SHELL_ARGV_DISPATCH_REVIEWS[rel_path]
            and any("$@" in token for token in segment)
        ):
            # These test helpers execute a quoted argv vector supplied by
            # literal call sites; they never reconstruct or re-evaluate shell.
            # (Helper test chạy argv được truyền trực tiếp; không ghép/eval shell.)
            findings.append({
                "file": rel_path,
                "line": line_no,
                "api": "shell-command",
                "executable": "<reviewed-argv-dispatch>",
                "review_status": "reviewed",
                "review_reason": "quoted argv dispatch is restricted to literal test call sites; no shell re-evaluation",
                "parse_error": None,
            })
            continue
        reviewed_dynamic = (
            rel_path in SHELL_EXPLICIT_DYNAMIC_REVIEWS
            and source_line in SHELL_EXPLICIT_DYNAMIC_REVIEWS[rel_path]
            and "$MGC_PATH" in segment
        )
        reviewed_source_inspection = (
            rel_path in SHELL_SOURCE_INSPECTION_REVIEWS
            and source_line in SHELL_SOURCE_INSPECTION_REVIEWS[rel_path]
        )
        reviewed_exact_shell = reviewed_dynamic or reviewed_source_inspection
        if segment and segment[0].replace("\\", "/").rsplit("/", 1)[-1] == "trap":
            executable = (
                "shell-trap-static"
                if _shell_trap_action_is_static(text, line_no, segment)
                else "<dynamic-shell-command>"
            )
        elif reviewed_exact_shell:
            executable = "<reviewed-dynamic-command>"
        else:
            executable = _shell_executable(
                segment,
                command_aliases,
                active_script_aliases,
                scanned_shell_paths,
            )
        if executable in SHELL_INTERPRETERS:
            targets = _shell_interpreter_script_target(
                segment, executable, active_script_aliases
            )
            verified_targets = targets.intersection(scanned_shell_paths)
            interpreter_index = next(
                (
                    index
                    for index, token in enumerate(segment)
                    if token.replace("\\", "/").rsplit("/", 1)[-1].lower() == executable
                ),
                len(segment),
            )
            startup_safe = True
            if executable == "bash":
                startup_safe = _shell_environment_unset_before(
                    segment, "BASH_ENV", interpreter_index
                )
            elif executable == "zsh":
                startup_safe = any(
                    re.fullmatch(r"-[A-Za-z]*f[A-Za-z]*", token)
                    for token in segment[interpreter_index + 1 :]
                )
            else:
                startup_safe = False
            executable = (
                "shell-script"
                if startup_safe and len(verified_targets) == 1 and len(targets) == 1
                else "<dynamic-shell-command>"
            )
        elif executable == "shell-script" and scanned_shell_sources:
            direct_targets = [
                (index, targets)
                for index, token in enumerate(segment)
                if (targets := _shell_script_argument_targets(token, active_script_aliases))
            ]
            script_index, target_paths = direct_targets[0] if direct_targets else (None, set())
            if len(direct_targets) != 1 or len(target_paths) != 1:
                executable = "<dynamic-shell-command>"
            else:
                target_source = scanned_shell_sources.get(next(iter(target_paths)), "")
                shebang = target_source.splitlines()[0].lower() if target_source else ""
                if "bash" in shebang:
                    if (
                        _shell_bash_shebang_is_login(shebang)
                        or not _shell_environment_unset_before(
                            segment, "BASH_ENV", script_index
                        )
                    ):
                        executable = "<dynamic-shell-command>"
                elif "zsh" in shebang:
                    executable = "<dynamic-shell-command>"
        if executable is None:
            continue
        dynamic = executable in {"<dynamic-shell-command>", "<unparsed-shell>"}
        dependency_action = (
            _shell_dependency_lifecycle_action(executable, segment, command_aliases)
            if not dynamic
            else None
        )
        fingerprint_payload = json.dumps(
            {
                "file": rel_path,
                "line": line_no,
                "source_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
                "control_context": _shell_control_context(text, line_no),
                "segment": segment,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
        fingerprint = hashlib.sha256(fingerprint_payload.encode("utf-8")).hexdigest()
        if dependency_action is not None:
            reviewed = (
                fingerprint in SHELL_DEPENDENCY_COMMAND_REVIEWS.get(rel_path, set())
                and fingerprint not in seen_dependency_fingerprints
            )
            seen_dependency_fingerprints.add(fingerprint)
            review_status = "reviewed" if reviewed else "unreviewed"
            review_reason = (
                f"exact dependency lifecycle command reviewed: {dependency_action}"
                if reviewed
                else f"dependency lifecycle command requires exact source review: {dependency_action}"
            )
        else:
            if reviewed_exact_shell:
                review_status = "reviewed"
                if reviewed_dynamic:
                    review_reason = (
                        "exact version/help argv is reviewed at the shell boundary; caller-selected binary behavior is outside this inventory"
                    )
                else:
                    review_reason = (
                        "exact source-inspection command reviewed; fixed find/grep pipeline inspects repository source without executing discovered names"
                    )
            else:
                review_status = "unreviewed" if dynamic else "observed"
                review_reason = (
                    None
                    if dynamic
                    else "literal process command is inventoried; no dependency lifecycle action matched"
                )
        findings.append({
            "file": rel_path,
            "line": line_no,
            "api": "shell-command",
            "executable": executable,
            "review_status": review_status,
            "review_reason": review_reason,
            "dependency_action": dependency_action,
            "command_fingerprint": fingerprint,
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


def scan_shell_file(abs_path: str, repo_root: str, scanned_shell_paths=None) -> list:
    """Read and inventory one shell file, failing closed on read errors.
    (Đọc và kiểm kê file shell; lỗi đọc phải xuất hiện trong ledger.)"""
    rel_path = os.path.relpath(abs_path, repo_root).replace(os.sep, "/")
    try:
        with open(abs_path, "r", encoding="utf-8") as handle:
            return scan_shell_text(rel_path, handle.read(), scanned_shell_paths)
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


def review_python_process_calls(calls: list, expected_files=None) -> list:
    """Apply exact AST review records and report stale/missing records.
    Áp dụng bản ghi review khớp AST chính xác; báo mục review cũ/bị thiếu.
    """
    expected_paths = (
        set(expected_files)
        if expected_files is not None
        else set(PYTHON_PROCESS_REVIEWS)
    )
    actual_counts = {}
    for item in calls:
        fingerprint = item.get("call_fingerprint")
        if fingerprint:
            key = (item["file"], item["api"], fingerprint)
            actual_counts[key] = actual_counts.get(key, 0) + 1

    seen_counts = {}
    reviewed = []
    for original in calls:
        item = dict(original)
        fingerprint = item.get("call_fingerprint")
        key = (item["file"], item["api"], fingerprint)
        path_reviews = PYTHON_PROCESS_REVIEWS.get(item["file"], {})
        review = path_reviews.get((item["api"], fingerprint))
        seen_counts[key] = seen_counts.get(key, 0) + 1
        if review and seen_counts[key] <= review[0]:
            item["review_status"] = "reviewed"
            item["review_reason"] = review[1]
        else:
            item["review_status"] = "unreviewed"
            item["review_reason"] = None
        reviewed.append(item)

    # A removed or changed reviewed call must not leave a stale approval in
    # the source allowlist; surface it as a blocking inventory record.
    for rel_path in sorted(expected_paths & PYTHON_PROCESS_REVIEWS.keys()):
        for (api, fingerprint), (expected_count, reason) in PYTHON_PROCESS_REVIEWS[rel_path].items():
            actual_count = actual_counts.get((rel_path, api, fingerprint), 0)
            if actual_count < expected_count:
                reviewed.append({
                    "file": rel_path,
                    "line": 0,
                    "api": api,
                    "executable": "<missing-reviewed-call>",
                    "call_fingerprint": fingerprint,
                    "review_status": "stale-review",
                    "review_reason": reason,
                    "parse_error": f"expected {expected_count} exact call(s), found {actual_count}",
                })
    return sorted(reviewed, key=lambda item: (item["file"], item["line"], item["api"]))


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
            "call_fingerprint": hashlib.sha256(
                ast.dump(node, include_attributes=False).encode("utf-8")
            ).hexdigest(),
            "review_status": "unreviewed",
            "review_reason": None,
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
    """Whether a function has a Rust test or test-only configuration attribute.
    (Xác định hàm có thuộc tính test Rust hoặc chỉ biên dịch khi test.)"""
    cursor = start_idx - 1
    while cursor >= 0:
        stripped = lines[cursor].strip()
        if not stripped or stripped.startswith("///") or stripped.startswith("//!"):
            cursor -= 1
            continue
        if stripped.startswith("#["):
            if re.match(r"#\[(?:tokio::)?test(?:\s*\(|\s*\])", stripped):
                return True
            if re.fullmatch(r"#\[cfg\(\s*test\s*\)\]", stripped):
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
                    route_key = (rel_path.replace(os.sep, "/"), fn_name, tool)
                    if pattern in EXECUTOR_ROUTE_PATTERNS:
                        classification_reason = AUDITED_EXECUTOR_ROUTES.get(route_key)
                    elif pattern in DIRECT_PROCESS_ROUTE_PATTERNS:
                        classification_reason = AUDITED_DIRECT_PROCESS_ROUTES.get(route_key)
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
    python_process_calls = review_python_process_calls([
        item
        for abs_path in python_files
        for item in scan_python_file(abs_path, repo_root)
    ], expected_files={os.path.relpath(path, repo_root) for path in python_files})
    shell_files = _iter_shell_files()
    powershell_files = _iter_files_with_suffixes((".ps1", ".psm1", ".psd1"))
    scanned_shell_sources = {}
    for path in [*shell_files, *powershell_files]:
        rel_path = os.path.relpath(path, repo_root).replace(os.sep, "/")
        try:
            with open(path, "r", encoding="utf-8") as handle:
                scanned_shell_sources[rel_path] = handle.read()
        except (OSError, UnicodeError):
            # The dedicated file scan emits the blocking read error below.
            # (Lượt quét file riêng sẽ ghi lỗi đọc thành finding chặn.)
            continue
    shell_process_calls = [
        item
        for abs_path in shell_files
        for item in scan_shell_file(abs_path, repo_root, scanned_shell_sources)
    ]
    observed_dependency_reviews = {
        (item["file"], item["command_fingerprint"])
        for item in shell_process_calls
        if item.get("dependency_action") is not None
    }
    for rel_path, fingerprints in SHELL_DEPENDENCY_COMMAND_REVIEWS.items():
        for fingerprint in sorted(fingerprints):
            if (rel_path, fingerprint) not in observed_dependency_reviews:
                shell_process_calls.append({
                    "file": rel_path,
                    "line": 0,
                    "api": "shell-command-review-record",
                    "executable": "<stale-review>",
                    "review_status": "unreviewed",
                    "review_reason": "review fingerprint no longer matches a live dependency command",
                    "dependency_action": None,
                    "command_fingerprint": fingerprint,
                    "parse_error": None,
                })
    javascript_files = _iter_files_with_suffixes((".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx"))
    javascript_process_calls = [
        item
        for abs_path in javascript_files
        for item in scan_supplemental_file(abs_path, repo_root, scan_javascript_text)
    ]
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
        "python_process_reviewed": sum(
            item["review_status"] == "reviewed" for item in python_process_calls
        ),
        "python_process_unreviewed": sum(
            item["review_status"] != "reviewed" for item in python_process_calls
        ),
        "shell_scan_roots": SHELL_SCAN_ROOTS,
        "shell_files_scanned": len(shell_files),
        "shell_process_unreviewed": sum(
            item["review_status"] == "unreviewed" for item in shell_process_calls
        ),
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
        f"reviewed={sum(item['review_status'] == 'reviewed' for item in python_process_calls)} "
        f"unreviewed={sum(item['review_status'] != 'reviewed' for item in python_process_calls)} "
        f"shell-process-surfaces={len(shell_process_calls)} "
        f"shell-dynamic={sum(item['review_status'] == 'unreviewed' for item in shell_process_calls)} "
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
    python_process_unreviewed = [
        item for item in python_process_calls
        if item["review_status"] != "reviewed"
    ]
    if python_process_unreviewed:
        print(
            "PYTHON PROCESS REVIEW REQUIRED — unreviewed or stale exact-call "
            "records remain in the ledger:",
            file=sys.stderr,
        )
        for item in python_process_unreviewed:
            print(
                f"  [{item['review_status']}] {item['file']}:{item['line']} "
                f"{item['api']} {item['executable']}",
                file=sys.stderr,
            )
        print(f"ledger: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
        return 1
    shell_process_unreviewed = [
        item for item in shell_process_calls
        if item["review_status"] == "unreviewed"
    ]
    if shell_process_unreviewed:
        print(
            "SHELL PROCESS REVIEW REQUIRED — dynamic/unparsed command heads or "
            "dependency lifecycle commands without an exact source review remain:",
            file=sys.stderr,
        )
        for item in shell_process_unreviewed:
            print(
                f"  [{item['review_status']}] {item['file']}:{item['line']} "
                f"{item['executable']}"
                + (
                    f" {item['dependency_action']}"
                    if item.get("dependency_action")
                    else ""
                ),
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
            "PROCESS AUDIT COVERAGE LIMITED — direct process boundaries are clean, "
            "but this result is not proof of complete repository-wide coverage:",
            file=sys.stderr,
        )
        for surface in UNSCANNED_PROCESS_SURFACES:
            print(f"  [unscanned] {surface['path']}: {surface['reason']}", file=sys.stderr)
        print(f"coverage details: {os.path.relpath(out_path, repo_root)}", file=sys.stderr)
    print(f"ledger: {os.path.relpath(out_path, repo_root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
