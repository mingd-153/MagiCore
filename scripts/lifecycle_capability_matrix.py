#!/usr/bin/env python3
"""Generate the LIFECYCLE capability matrix from evidence — separate
from the audit capability matrix (Tech Lead P0-2, 2026-09-11).

The audit matrix (audit_capability_matrix.py) proves ADVISORY evidence
per lane (clean/vulnerable/tool-failure). It can NEVER justify a
"language supported" claim about the PRODUCT — lifecycle support is a
separate dimension with its own evidence: create → detect → resolve →
lock → fetch → verify → install/materialize → add/remove/update/list →
frozen/offline reinstall → store/GC → test → build → run → dev → audit →
optimizer → recovery.

Schema v6 (framework-qualified lane identity added 2026-09-29): 23
dimensions, each recorded in a
SIX-VALUE status vocabulary — native-pass (mgc itself owns the
capability), managed-delegation-pass (mgc orchestrates a toolchain it
manages, e.g. redirected CARGO_HOME/UV_CACHE_DIR), plain-delegation-pass
(mgc calls through to an unmanaged toolchain), unverified (skipped or
environment missing — NEVER auto-promoted to a pass), unsupported (the
dimension does not exist for this lane), failed (crash/step failure).
Only the three *-pass statuses can satisfy a required dimension:
fail-closed by construction.

This script executes the real `mgc` binary through per-lane lifecycle
steps in a sandbox and records which dimensions each lane actually
completed (passed) vs. which are delegated/unsupported/failed. The
output drives release gates that need PRODUCT capability, not audit
capability. Fail-closed: any binary crash/unparseable step aborts.

Sinh ma trận năng lực LIFECYCLE từ bằng chứng — tách khỏi matrix audit
(P0-2). Matrix audit chứng minh evidence advisory từng lane, KHÔNG BAO
GIỜ suy ra được năng lực product; lifecycle là dimension riêng với
evidence riêng: create → detect → resolve → lock → fetch → verify →
install/materialize → add/remove/update/list → frozen/offline reinstall →
store/GC → test → build → run → dev → audit → optimizer → recovery.

Schema v6: 23 dimension, mỗi dimension ghi bằng MỘT TRONG
SÁU trạng thái — native-pass (mgc tự giữ năng lực), managed-delegation-
pass (mgc điều phối toolchain mình quản, vd CARGO_HOME/UV_CACHE_DIR
chuyển hướng), plain-delegation-pass (mgc gọi-through toolchain không
quản), unverified (bị skip hoặc thiếu môi trường — KHÔNG BAO GIỜ tự
động thành pass), unsupported (dimension không tồn tại cho lane),
failed (crash/bước fail). Chỉ ba trạng thái *-pass thỏa được dimension
bắt buộc: fail-closed ngay từ cấu trúc. Script chạy binary mgc THẬT qua
từng bước lifecycle trong sandbox rồi ghi dimension nào lane đó hoàn
thành thật. Fail-closed: binary crash/step không parse được → huỷ.
"""

import datetime
import time
import json
import os
import platform
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Optional


def run_text_capture(*args, **kwargs):
    """Run a child process with stable UTF-8 decoding for captured text.
    Chạy tiến trình con với giải mã UTF-8 ổn định cho output dạng text.
    """
    kwargs.setdefault("text", True)
    kwargs.setdefault("encoding", "utf-8")
    kwargs.setdefault("errors", "replace")
    return subprocess.run(*args, **kwargs)


def github_actions_annotation_escape(value: object) -> str:
    """Escape untrusted text before writing a GitHub workflow command.
    (Escape text không tin cậy trước khi ghi thành workflow command GitHub.)
    """
    return str(value).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def github_actions_error_annotations(failures: list[str]) -> list[str]:
    """Format at most ten actionable failure annotations for one step.
    (Tạo tối đa mười annotation lỗi có ích cho một step.)
    """
    if len(failures) > 10:
        messages = failures[:9] + [
            f"{len(failures) - 9} additional gate failures; full details are in this step log"
        ]
    else:
        messages = failures
    return [
        "::error::" + github_actions_annotation_escape(message)
        for message in messages
    ]


ALL_DEPENDENCY_OPERATIONS = (
    "install", "add", "remove", "update", "list", "resolve", "lock",
    "fetch", "verify", "store", "materialize", "frozen-install",
    "offline-reinstall", "gc",
)
NATIVE_INSTALL_PIPELINE_OPERATIONS = frozenset(
    {"resolve", "lock", "fetch", "verify", "store", "materialize", "frozen-install"}
)
NATIVE_OPERATIONS_EXCEPT_GC = frozenset(ALL_DEPENDENCY_OPERATIONS) - {"gc"}
NATIVE_OPERATIONS_EXCEPT_GC_AND_OFFLINE = frozenset(ALL_DEPENDENCY_OPERATIONS) - {
    "gc", "offline-reinstall"
}

# Lifecycle lanes and their steps. Each step is an argv list run with
# cwd = the sandbox project dir. Steps are ordered; a lane records the
# deepest step reached. "delegated" steps (native toolchain owns the
# behavior, mgc orchestrates) are recorded as delegated, not passed.
# Các lane lifecycle và bước của nó — mỗi bước là argv chạy với cwd =
# thư mục project sandbox; lane ghi lại bước sâu nhất đạt tới. Bước
# "delegated" (toolchain gốc giữ behavior, mgc điều phối) ghi là
# delegated chứ không ghi passed.
LANES = [
    {
        "core": "lib",
        "language": "rust",
        "framework_ids": ["rust"],
        "framework_id": "rust",
        "scaffold": ["create-lib", "rust", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        # cargo owns fetch/test/build; mgc orchestrates via mgc.lock.
        # `run` is an npm-script concept — honest absent for lib/rust.
        # cargo giữ fetch/test/build; mgc điều phối qua mgc.lock. `run`
        # là khái niệm npm-script — absent trung thực cho lib/rust.
        "delegated": [],
        # Dimensions this lane MUST pass for "orchestration-lifecycle-passed" —
        # the verdict contract is per-lane, not global (P0 finding #1).
        # Dimension lane này BẮT BUỘC pass để "orchestration-lifecycle-passed" —
        # hợp đồng verdict theo từng lane, không theo tập toàn cục (P0
        # finding #1).
        "required_dims": ["create", "install", "test", "build"],
        # Phase 2 (2026-09-16): lib/rust resolves+fetches+materializes via
        # the native crates.io engine (mgc-resolver) — mgc OWNS the install
        # lifecycle, no `cargo fetch` spawn. Verdict stays honest (evidence:
        # unit + mockito; runtime lane not yet proven in this matrix).
        # (Phase 2: lib/rust resolve+fetch+materialize qua engine crates.io
        # native (mgc-resolver) — mgc GIỮ lifecycle install, không spawn
        # `cargo fetch`. Verdict giữ trung thực (evidence: unit + mockito;
        # runtime lane chưa chứng minh trong matrix này).)
        "install_owner": "native-engine",
        # Native crates.io engine exists (unit + mockito evidence).
        # (Engine crates.io native tồn tại (evidence unit + mockito).)
        "dependency_owner": "mgc-native",
        "note": "native-engine: exists, evidence: unit+mockito",
        "owner_by_operation": {
            "resolve": "mgc",                # mgc sparse-index engine resolves
            "lock": "mgc",                   # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                  # mgc downloads the .crate
            "verify": "mgc",                 # mgc checks crates.io checksum — mgc xác minh checksum crates.io
            "store": "magicore-shared-cas",  # blake3 CAS import
            "materialize": "mgc",            # mgc writes registry/{cache,src} layout
        },
    },
    {
        "core": "lib",
        "language": "python",
        "framework_ids": ["python"],
        "framework_id": "python",
        "scaffold": ["create-lib", "python", "test-lib"],
        # lib/python needs pytest (mgc test) and build (python -m build)
        # provisioned before its lifecycle steps run.
        # lib/python cần pytest (mgc test) và build (python -m build)
        # được provision trước khi chạy các bước lifecycle.
        "pre_steps": [
            "provision_py_build_tools",
        ],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        # Phase 2 (2026-09-16): lib/python resolves+fetches+materializes via
        # the native PyPI engine (mgc-resolver) — mgc OWNS the install
        # lifecycle, no `uv sync`/`pip install` spawn. Verdict stays honest
        # (evidence: unit + mockito).
        # (Phase 2: lib/python resolve+fetch+materialize qua engine PyPI
        # native (mgc-resolver) — mgc GIỮ lifecycle install, không spawn
        # `uv sync`/`pip install`. Verdict giữ trung thực (unit + mockito).)
        "install_owner": "native-engine",
        # Native PyPI engine exists (unit + mockito evidence).
        # (Engine PyPI native tồn tại (evidence unit + mockito).)
        "dependency_owner": "mgc-native",
        "note": "native-engine: exists, evidence: unit+mockito",
        "owner_by_operation": {
            "resolve": "mgc",                # mgc JSON API engine resolves
            "lock": "mgc",                   # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                  # mgc downloads wheel/sdist
            "verify": "mgc",                 # mgc checks PyPI wheel digests — mgc xác minh digest wheel PyPI
            "store": "magicore-shared-cas",  # blake3 CAS import
            "materialize": "mgc",            # mgc writes pypi/wheels layout
        },
    },
    {
        "core": "lib",
        "language": "typescript",
        "framework_ids": ["ts"],
        "framework_id": "ts",
        "scaffold": ["create-lib", "typescript", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        # TypeScript rides the WEB engine (mgc resolve+lock+fetch+CAS)
        # — a native-engine candidate (P0-D).
        # (TypeScript đi trên engine WEB (mgc resolve+lock+fetch+CAS)
        # — ứng viên native-engine (P0-D).)
        "install_owner": "native-engine",
        "recovery_probe": True,
        # P0-D: the web engine IS mgc — lib/typescript is mgc-native.
        # (P0-D: engine web CHÍNH LÀ mgc — lib/typescript là mgc-native.)
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # web engine resolves the graph
            "lock": "mgc",                   # mgc.lock
            "fetch": "mgc",                  # mgc fetcher + CAS
            "verify": "mgc",                 # web engine checks registry integrity — web engine xác minh integrity registry
            "store": "magicore-shared-cas",  # global shared CAS
            "materialize": "mgc",            # mgc materializes node_modules
        },
    },
    {
        "core": "lib",
        "language": "go",
        "framework_ids": ["go"],
        "framework_id": "go",
        "scaffold": ["create-lib", "go", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        # No delegated lifecycle steps (native install; test/build run
        # under mgc's own exec, same precedent as lib/python).
        "delegated": [],
        # Phase 2 native (2026-09-16): the GoMod proxy engine
        # (protocols/go.rs) resolves/downloads/verifies modules — install
        # is mgc-owned now; go toolchain remains for build/test only.
        # (Phase 2 native: engine GoMod proxy (protocols/go.rs) tự
        # resolve/download/verify module — install là của mgc; go toolchain
        # chỉ còn build/test.)
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # GoMod proxy engine resolves the graph
            "lock": "mgc",                   # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                  # mgc downloads @v zips
            "verify": "mgc",                 # mgc checks ziphash/sumdb — mgc xác minh ziphash/sumdb
            "store": "magicore-shared-cas",  # blake3 CAS import
            "materialize": "mgc",            # GOMODCACHE layout for offline builds
        },
    },
    {
        "core": "lib",
        "language": "java",
        "framework_ids": ["java"],
        "framework_id": "java",
        # Maven dependency resolution is native, but Java test/build execution
        # has no MGC-owned backend yet; retain its evidence without release claims.
        # (Resolver Maven đã native, nhưng Java chưa có backend test/build do MGC giữ.)
        "evidence_only": True,
        "scaffold": ["create-lib", "java", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        # No delegated lifecycle steps (native install; test/build run
        # under mgc's own exec, same precedent as lib/python).
        "delegated": [],
        # Phase 2 native (2026-09-16): the Maven engine (protocols/maven.rs)
        # resolves POM graphs and verifies jars (sha256/sha1) — install is
        # mgc-owned for pom.xml projects; gradle stays a build-lane tool.
        # (Phase 2 native: engine Maven (protocols/maven.rs) tự resolve graph
        # POM và verify jar (sha256/sha1) — install là của mgc cho project
        # pom.xml; gradle chỉ là tool build-lane.)
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # Maven engine resolves the POM graph
            "lock": "mgc",                   # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                  # mgc downloads jars
            "verify": "mgc",                 # mgc checks published SHA-256/SHA-1 — mgc xác minh SHA-256/SHA-1 đã publish
            "store": "magicore-shared-cas",  # sha256/sha1-verified CAS import
            "materialize": "mgc",            # M2 repository layout for offline builds
        },
    },
    {
        "core": "lib",
        "language": "dotnet",
        "framework_ids": ["dotnet"],
        "framework_id": "dotnet",
        # NuGet resolve/fetch/install is native, but build/test cannot run
        # without MSBuild's restore-generated assets. MGC does not generate
        # that state, and §5.1 does not allow the dotnet executable.
        # Resolve/fetch/install NuGet là native, nhưng build/test cần assets
        # do MSBuild restore tạo. MGC chưa sinh state này và §5.1 chưa cho
        # phép chạy executable dotnet.
        "evidence_only": True,
        "scaffold": ["create-lib", "dotnet", "test-lib"],
        "steps": [
            ("install", ["install"]),
        ],
        "lifecycle_owner_overrides": {
            "test": "unsupported",
            "build": "unsupported",
        },
        "delegated": [],
        # Phase 2 native (2026-09-16): the NuGet v3 engine
        # (protocols/nuget.rs) resolves flat-container versions and
        # verifies SHA-512 nupkgs — install is mgc-owned.
        # (Phase 2 native: engine NuGet v3 (protocols/nuget.rs) tự resolve
        # flat-container versions và verify SHA-512 nupkg — install là của
        # mgc.)
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",               # NuGet v3 engine resolves
            "lock": "mgc",                  # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                 # mgc downloads nupkgs
            "verify": "mgc",                # mgc checks NuGet hashes — mgc xác minh hash NuGet
            "store": "magicore-shared-cas", # sha512-verified CAS import
            "materialize": "mgc",           # global-packages layout for offline restore
        },
    },
    {
        "core": "web",
        "language": "javascript",
        "scaffold": ["create-web", "react", "test-web"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
            # `dev` is PROBED (dev_probe below), not stepped: a
            # long-lived server can only be proven by SERVING + HMR
            # roundtrip, never by `--help` exit-0 (which only proves
            # Clap wiring — Tech Lead P0-mới-1 vòng-6/7).
            # `dev` được PROBE (dev_probe bên dưới), không chạy bước đơn:
            # server dài hạn chỉ chứng minh được bằng PHỤC VỤ + HMR
            # roundtrip, không bao giờ bằng exit-0 của `--help` (chỉ
            # chứng minh Clap wiring — Tech Lead P0-mới-1).
        ],
        "delegated": [],
        # Web carries the product's flagship promise — run + dev are part
        # of the lane's own lifecycle contract (P0 finding #1: the
        # verdict must weigh every dimension the lane declares).
        # Web giữ lời hứa chủ lực của product — run + dev thuộc hợp đồng
        # lifecycle của chính lane (P0 finding #1: verdict phải cân mọi
        # dimension lane tuyên bố).
        "required_dims": ["create", "install", "test", "build", "dev", "run"],
        # The WEB ENGINE (mgc itself) owns resolve+lock+fetch+CAS — the
        # only native-engine install today (P0-D taxonomy).
        # (Engine WEB (chính mgc) giữ resolve+lock+fetch+CAS — install
        # native-engine duy nhất hôm nay (taxonomy P0-D).)
        "install_owner": "native-engine",
        "lifecycle_owner_overrides": {"dev": "mgc-native"},
        "recovery_probe": True,
        # P0-D: the web engine IS mgc — the flagship mgc-native lane.
        # (P0-D: engine web CHÍNH LÀ mgc — lane mgc-native chủ lực.)
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # web engine resolves the graph
            "lock": "mgc",                   # mgc.lock
            "fetch": "mgc",                  # mgc fetcher + CAS
            "verify": "mgc",                 # web engine checks registry integrity — web engine xác minh integrity registry
            "store": "magicore-shared-cas",  # global shared CAS
            "materialize": "mgc",            # mgc materializes node_modules
        },
        # `run` proves the production entry (`mgc run start` → vite
        # preview): a long-lived server, so the probe starts it, waits
        # for an HTTP roundtrip, then kills it cleanly — the dimension
        # passes when the server SERVED, not merely started.
        # `run` chứng minh entry production (`mgc run start` → vite
        # preview): server dài hạn nên probe khởi động, chờ HTTP
        # roundtrip, rồi tắt sạch — dimension pass khi server ĐÃ PHỤC
        # VỤ, không chỉ khởi động.
        "run_probe": {
            "script": "start",
            "port": 4315,
        },
        # `dev` probe (P0-mới-1, Tech Lead vòng-6/7): the OLD step was
        # `["dev", "--help"]` — exit-0 proved ONLY the Clap wiring while
        # the lane still claimed a `dev` dimension. The real probe spawns
        # `mgc dev`, waits for the port to bind + HTTP 200, edits a
        # source file, expects NEW server output (rebuild), checks the
        # HMR websocket endpoint, then kills the tree and sweeps the
        # port. dev passes only when the server SERVED + rebuilt.
        # Probe `dev` (P0-mới-1): bước cũ `["dev","--help"]` — exit-0 chỉ
        # chứng minh Clap wiring trong khi lane vẫn claim dimension dev.
        # Probe thật spawn `mgc dev`, chờ port bind + HTTP 200, sửa file
        # source, chờ output mới (rebuild), kiểm tra websocket HMR, rồi
        # kill cây process và quét port. dev chỉ pass khi server ĐÃ PHỤC
        # VỤ + rebuild.
        "dev_probe": {
            "port": 4315,
            "hmr_path": "/@magicore/hmr",
            "source_file": "src/App.tsx",
            "source_edit": "// dev-probe touch — marker HMR rebuild\n",
        },
    },
    {
        "core": "web",
        "language": "vanilla",
        "framework_ids": ["vanilla"],
        "framework_id": "vanilla",
        "scaffold": ["create-web", "vanilla", "test-web-vanilla"],
        "required_markers": ["index.html"],
        "steps": [("install", ["install"]), ("test", ["test"]), ("build", ["build"])],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {},
    },
    {
        "core": "web",
        "language": "ts",
        "framework_ids": ["ts"],
        "framework_id": "ts",
        "scaffold": ["create-web", "vanilla", "test-web-ts", "--ts"],
        "project_dir": "test-web-ts",
        "required_markers": ["index.html", "src/main.ts", "tsconfig.json"],
        "steps": [("install", ["install"]), ("test", ["test"]), ("build", ["build"])],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {},
    },
    {
        "core": "web",
        "language": "node",
        "framework_ids": ["node"],
        "framework_id": "node",
        "scaffold": ["create-web", "express", "test-web-node"],
        "steps": [("install", ["install"]), ("test", ["test"]), ("build", ["build"])],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {},
    },
    {
        "core": "ai",
        "language": "python",
        "scaffold": ["create-ai", "python-agent", "test-ai"],
        # Native (mgc.lock, no uv.lock): the lane adds a REAL direct
        # dependency (six, universal py2.py3-none-any wheel) via `mgc add-ai`
        # (native PyPI resolve-first + mgc-side pyproject edit) BEFORE install, so the
        # lockfile carries a genuine resolve + hash and `mgc install-ai`
        # must actually materialize it — no uv pre-steps, no uv.lock.
        # (Native: thêm dep thật bằng `mgc add-ai`, không uv.)
        "dependency_fixture": {
            "command": "add-ai",
            "package": "six@1.17.0",
        },
        "pre_steps": [
            "mgc_add_real_dependency",
            # pytest runs `mgc test`; `build` runs `python -m build`.
            # Both are provisioned BEFORE the lifecycle so the lane
            # never skips on a missing tool.
            # pytest chạy `mgc test`; `build` chạy `python -m build`.
            # Cả hai được provision TRƯỚC lifecycle để lane không bao
            # giờ skip vì thiếu tool.
            "provision_py_build_tools",
        ],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        # AI lane: the scaffold itself ships NO dependency set — the
        # install dimension is proven against a lane-injected fixture
        # (added via `mgc add-ai`, native).
        # Lane AI: scaffold không kèm dependency — dimension install
        # được chứng minh trên fixture lane tự thêm (qua `mgc add-ai`).
        "required_dims": ["create", "install", "test", "build"],
        "install_dim_name": "install-command-empty-project",
        # Native PyPI engine owns resolve/fetch/install (mgc.lock) —
        # managed-delegation record retired with the uv pre-steps.
        # (Engine PyPI native giữ resolve/fetch/install.)
        "install_owner": "native-engine",
        # mgc owns the ai/python dependency lifecycle (native).
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # PyPI JSON API engine resolves
            "lock": "mgc",                   # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                  # mgc downloads wheels
            "verify": "mgc",                 # mgc checks wheel digest/RECORD — mgc xác minh digest/RECORD của wheel
            "store": "magicore-shared-cas",  # verified CAS import
            "materialize": "mgc",            # unpacked site dirs for import
        },
    },
    {
        "core": "app",
        "language": "flutter",
        "scaffold": ["create-app", "flutter", "test-app"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        # Native (mgc.lock, no uv.lock): install runs the shared PyPI
        # engine (parse → resolve → verified fetch → mgc.lock); test and
        # build run under mgc's own exec. No uv anywhere on this lane.
        # (Native: install qua engine PyPI, không uv.)
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        # mgc owns the app/flutter dependency lifecycle (native).
        "dependency_owner": "mgc-native",
        "note": "native pub.dev engine owns resolve/fetch/install (mgc.lock + pub cache); test/build run under mgc exec",
        "owner_by_operation": {
            "resolve": "mgc",                # pub.dev JSON API engine resolves
            "lock": "mgc",                   # mgc.lock v3 (native-resolve)
            "fetch": "mgc",                  # mgc downloads archives
            "verify": "mgc",                 # mgc checks pub archive SHA-256 — mgc xác minh SHA-256 archive pub
            "store": "magicore-shared-cas",  # verified CAS import
            "materialize": "mgc",            # pub cache layout for offline builds
        },
    },
    # ===== Phase 2 native app lanes (2026-09-16) — evidence-only until
    # release E2E exists. SwiftPM engine (protocols/swift.rs), RN layered
    # engine (protocols/reactnative.rs: JS web delegate + gradle.lockfile
    # -> Maven + Podfile.lock -> CocoaPods CDN).
    # (Phase 2 native app lanes — chỉ-ghi-bằng-chứng cho tới khi có release
    # E2E. Engine SwiftPM (protocols/swift.rs), engine RN layered
    # (protocols/reactnative.rs: JS delegate web + gradle.lockfile ->
    # Maven + Podfile.lock -> CocoaPods CDN).) =====
    {
        "core": "app",
        "language": "swift",
        "evidence_only": True,
        "toolchain_probes": ["swift"],
        "scaffold": ["create-app", "swift", "test-app"],
        "steps": [("install", ["install"])],
        "required_dims": ["create", "install"],
        # The app CLI now routes registry dependencies through MGC's Swift
        # resolver/fetch/verify/materializer. Source/Git dependencies remain
        # explicitly unsupported; install ownership is not full ecosystem parity.
        # (CLI app dùng resolver/fetch/verify/materializer Swift của MGC;
        # dependency source/Git vẫn unsupported.)
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "note": "MGC-native Swift registry install; add/remove/update/list remain unsupported; Git dependencies are rejected",
        "owner_by_operation": {},
    },
    {
        "core": "app",
        "language": "objc",
        "evidence_only": True,
        "toolchain_probes": ["pod"],
        "scaffold": ["create-app", "objc", "test-app"],
        "steps": [("install", ["install"])],
        "required_dims": ["create", "install"],
        # Objective-C dependency install is rejected before external tools.
        # (Cài dependency Objective-C bị từ chối trước khi gọi tool ngoài.)
        "install_owner": "unsupported",
        "dependency_owner": "unsupported",
        "note": "unsupported: no MGC-native ObjC resolver/materializer and no compatibility spawn in this lane",
        "owner_by_operation": {},
    },
    {
        "core": "app",
        "language": "react-native",
        "evidence_only": True,
        "toolchain_probes": ["node"],
        "scaffold": ["create-app", "react-native", "test-app"],
        "steps": [("install", ["install"])],
        "required_dims": ["create", "install"],
        # C0 (T0.3, 2026-09-17): React Native has PER-TIER ownership (no
        # single-row native label): JS tier rides the web pipeline, Android
        # tier resolves via the native Maven engine, iOS tier verifies
        # Podfile.lock via the CocoaPods CDN engine — but the invoked
        # `install-app` lane has NO runner and errors before any spawn, so
        # the lane as-invoked supports no install lifecycle: unsupported.
        # (C0: React Native sở hữu PER-TIER (không nhãn native đơn dòng):
        # lane install không có runner nên lỗi — unsupported.)
        "install_owner": "unsupported",
        # P0-D/T0.4: no install lifecycle exists on the invoked lane.
        "dependency_owner": "unsupported",
        "note": "per-tier: js=web-pipeline, android=Maven-engine, ios=CocoaPods-CDN-verify; invoked lane has no runner and errors (fail-closed); iOS tier fail-closed without Podfile.lock",
        "owner_by_operation": {},
    },
    # ===== Lifecycle evidence-only lanes (P0-6, Tech Lead 2026-09-15) =====
    # These lanes record lifecycle evidence without gating lifecycle
    # promotion. Missing external toolchains produce `unverified` dimensions
    # and `evidence-unverified`, never a pass. Owner taxonomy remains declared;
    # MGC-native package claims still require the independent native-PM gate.
    # (Các lane này ghi lifecycle evidence nhưng không chặn lifecycle
    # promotion. Thiếu toolchain ngoài ghi dimension `unverified` và verdict
    # `evidence-unverified`, không giả pass. Owner vẫn phải khai; claim package
    # MGC-native vẫn phải qua gate native-PM riêng.)
    {
        "core": "game",
        "language": "rust",
        "evidence_only": True,
        # bevy lane: cargo is the whole toolchain (probe), the scaffold
        # writes a Cargo.toml (marker "rust" already proves it).
        # (lane bevy: cargo là toàn bộ toolchain (probe), scaffold ghi
        # Cargo.toml — marker "rust" chứng minh sẵn.)
        "toolchain_probes": ["cargo"],
        "scaffold": ["create-game", "bevy", "test-game"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        # The CLI routes dependency installation through MGC's Lib/Rust
        # engine; Cargo remains a compiler/build boundary, not the installer.
        # (CLI route cài dependency qua engine Lib/Rust của MGC; Cargo chỉ
        # là ranh giới compiler/build, không phải installer.)
        "dependency_owner": "mgc-native",
        "note": "CLI routes Bevy dependencies through MGC Lib/Rust resolver; build/test still use compiler toolchain",
        "owner_by_operation": {},
    },
    {
        "core": "iot",
        "language": "rust",
        "evidence_only": True,
        # esp32-rust lane: scaffold emits an embedded-rust Cargo.toml; the
        # full cross toolchain (xtensa/riscv target) may still be missing —
        # later steps fail honestly if so.
        # (lane esp32-rust: scaffold sinh embedded-rust Cargo.toml; cross
        # toolchain đầy đủ (target xtensa/riscv) có thể vẫn thiếu — các
        # bước sau fail trung thực nếu vậy.)
        "toolchain_probes": ["cargo"],
        "scaffold": ["create-iot", "esp32-rust", "test-iot"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        # ESP32-Rust's CLI dependency route uses MGC's Lib/Rust engine;
        # PlatformIO/Zephyr lanes remain unsupported.
        # (ESP32-Rust dùng engine Lib/Rust của MGC; PlatformIO/Zephyr
        # vẫn unsupported.)
        "dependency_owner": "mgc-native",
        "note": "ESP32-Rust dependencies route through MGC Lib/Rust resolver; board compiler/toolchain is a separate build boundary",
        "owner_by_operation": {},
    },
    {
        "core": "clo",
        "language": "terraform",
        "evidence_only": True,
        # cloud/terraform lane: terraform CLI is external and usually absent
        # on runners — the probe records unverified instead of faking a run.
        # (lane cloud/terraform: CLI terraform là bên ngoài, thường vắng
        # trên runner — probe ghi unverified thay vì giả một lần chạy.)
        "toolchain_probes": ["terraform"],
        "scaffold": ["create-clo", "terraform", "test-cloud"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "unsupported",
        # The C0 gate rejects Terraform dependency operations; do not label
        # the lane as delegated merely because Terraform exists upstream.
        # (C0 từ chối dependency Terraform; không gắn delegated chỉ vì có
        # tool Terraform ở upstream.)
        "dependency_owner": "unsupported",
        "owner_by_operation": {},
    },
    {
        "core": "clo",
        "language": "cdk",
        "framework_ids": ["cdk"],
        "framework_id": "cdk",
        "scaffold": ["create-clo", "cdk", "test-cloud-cdk"],
        "steps": [("install", ["install"]), ("test", ["test"]), ("build", ["build"])],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {},
    },
    {
        "core": "clo",
        "language": "pulumi",
        "framework_ids": ["pulumi"],
        "framework_id": "pulumi",
        "scaffold": ["create-clo", "pulumi", "test-cloud-pulumi"],
        "toolchain_probes": ["pulumi"],
        "pre_steps": ["initialize_local_pulumi_stack", "mgc_add_real_dependency"],
        # Pulumi's Node.js SDK must be in the package manifest before its
        # CLI can preview the program; use MGC's native Cloud/Web resolver
        # only when the generated template did not already declare it.
        # (SDK Node.js của Pulumi phải có trong manifest trước khi CLI
        # preview; chỉ dùng resolver Cloud/Web native của MGC khi template
        # chưa khai báo sẵn.)
        "dependency_fixture": {
            "command": "add-clo",
            "package": "@pulumi/pulumi@^3.0.0",
            "manifest_path": "package.json",
            "manifest_name": "@pulumi/pulumi",
            "skip_if_declared": True,
        },
        "steps": [("install", ["install"]), ("test", ["test"]), ("build", ["build"])],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "native-engine",
        "dependency_owner": "mgc-native",
        "owner_by_operation": {},
    },
    {
        "core": "hardware",
        "language": "benchmark",
        "evidence_only": True,
        # hardware lane: the core's frameworks are `optimizer`/`bench` —
        # mgc-native benchmark tooling, no FPGA vendor toolchain exists in
        # this core yet. No external probe (mgc owns the harness); the
        # `fpga` toolchain question stays open and honestly unclaimed.
        # (lane hardware: framework của core là `optimizer`/`bench` — công
        # cụ benchmark của mgc, CHƯA có toolchain FPGA vendor nào ở core
        # này. Không probe ngoài (mgc giữ harness); câu hỏi toolchain `fpga`
        # còn mở và trung thực không claim.)
        "toolchain_probes": [],
        "scaffold": ["create-hardware", "bench", "test-hw"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        # P0-5 (Tech Lead verdict 2026-09-16): the previous `native-engine`
        # + mgc-owned resolve/fetch claim was a taxonomy FALSE-POSITIVE —
        # the production adapter returns MgError::Unsupported for the whole
        # registry surface: HardwareAdapter::resolve (adapter.rs:55),
        # ::fetch (adapter.rs:67), ::install (adapter.rs:77),
        # ::update (adapter.rs:111), ::write_manifest (adapter.rs:41).
        # Hardware is a SCAFFOLD/GENERATOR-ONLY core: `mgc add-hardware
        # <pkg>` materializes bundled templates into optimizer/ + bench/,
        # nothing ever resolves or fetches. Of the three owner labels this
        # is the most honest: the ONLY machinery that materializes is mgc's
        # own in-tree, mgc-managed template generator — no unmanaged
        # external toolchain exists (toolchain_probes below is empty), so
        # plain-delegation would be literally false; managed-delegation
        # captures "mgc manages the machinery" without claiming the
        # registry pipeline. Per-operation owners name the truth: registry
        # ops are `unsupported-…`, store/materialize name the generator.
        # (P0-5: claim cũ `native-engine` + resolve/fetch của mgc là
        # false-positive taxonomy — adapter production trả
        # MgError::Unsupported cho toàn bộ mặt registry: resolve/fetch/
        # install/update/write_manifest (adapter.rs:55/67/77/111/41).
        # hardware là core CHỈ-SCAFFOLD/GENERATOR: `mgc add-hardware <pkg>`
        # materialize template bundled vào optimizer/ + bench/, không gì
        # resolve hay fetch cả. Trong 3 nhãn hiện có đây là nhãn trung thực
        # nhất: cơ khí DUY NHẤT materialize là template generator trong-
        # tree do chính mgc quản — không tồn tại toolchain ngoài nào
        # (toolchain_probes bên dưới rỗng), nên plain-delegation sẽ sai
        # đen; managed-delegation bắt được nghĩa "mgc quản cơ khí" mà
        # không claim pipeline registry. Owner per-operation nói thẳng:
        # op registry là `unsupported-…`, store/materialize nêu generator.)
        "install_owner": "unsupported",
        # P0-D: hardware is a scaffold/generator core (P0-5 — the adapter
        # returns Unsupported for the whole registry surface): the only
        # materialization is mgc's own bundled-template generator, so the
        # honest owner is scaffold-only, NOT delegated (no external
        # toolchain) and NOT mgc-native (no registry lifecycle).
        # (P0-D: hardware là core scaffold/generator (P0-5 — adapter trả
        # Unsupported cho toàn bộ mặt registry): cơ khí duy nhất là
        # generator template bundled của mgc, nên owner trung thực là
        # scaffold-only, KHÔNG phải delegated (không toolchain ngoài) và
        # KHÔNG phải mgc-native (không lifecycle registry).)
        "dependency_owner": "scaffold-only",
        "owner_by_operation": {},
    },
    {
        "core": "cicd",
        "language": "github-actions",
        "evidence_only": True,
        # github-actions lane: the scaffold writes .github/workflows/ci.yml
        # (mgc-native file generation, marker below); lifecycle install/test/
        # build steps record what actually happens today — evidence, not a
        # gate.
        # (lane github-actions: scaffold ghi .github/workflows/ci.yml (sinh
        # file bởi mgc — marker dưới); các bước install/test/build ghi đúng
        # cái xảy ra hôm nay — bằng chứng, không phải gate.)
        "toolchain_probes": [],
        "scaffold": ["create-cicd", "github-actions", "test-cicd"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        # P0-5 (Tech Lead verdict 2026-09-16): the previous `native-engine`
        # + mgc-owned resolve/fetch claim was a taxonomy FALSE-POSITIVE —
        # the production adapter returns MgError::Unsupported for the
        # registry surface: CicdAdapter::resolve (adapter.rs:65), ::fetch
        # (adapter.rs:75), ::install (adapter.rs:83), ::write_manifest
        # (adapter.rs:52), and ::add/::remove/::update all fail with
        # "cicd has no package manager". CI/CD is a GENERATOR lane: the
        # scaffold writes .github/workflows/ci.yml once, pipeline files are
        # HAND-OWNED afterwards (adapter guidance), and execution lives on
        # the CI provider — mgc manages no cache or toolchain here. Of the
        # three owner labels this is the most honest: everything past the
        # scaffold belongs to an unmanaged external system (the provider +
        # humans), which is exactly plain-delegation. Per-operation owners
        # name the truth: registry ops are `unsupported-…`, store/
        # materialize belong to the CI provider.
        # (P0-5: claim cũ `native-engine` + resolve/fetch của mgc là
        # false-positive taxonomy — adapter production trả
        # MgError::Unsupported cho mặt registry: resolve/fetch/install/
        # write_manifest (adapter.rs:65/75/83/52), add/remove/update đều
        # fail "cicd has no package manager". cicd là lane GENERATOR:
        # scaffold ghi .github/workflows/ci.yml một lần, file pipeline do
        # NGƯỜI quản sau đó (guidance của adapter), thực thi nằm ở CI
        # provider — mgc không quản cache/toolchain nào ở đây. Trong 3
        # nhãn đây là nhãn trung thực nhất: mọi thứ sau scaffold thuộc hệ
        # thống ngoài không quản lý (provider + con người) — đúng nghĩa
        # plain-delegation. Owner per-operation nói thẳng: op registry là
        # `unsupported-…`, store/materialize thuộc CI provider.)
        "install_owner": "unsupported",
        # P0-D: cicd is a generator lane (P0-5 — the adapter returns
        # Unsupported for the registry surface; pipelines are hand-owned
        # afterwards): scaffold-only.
        # (P0-D: cicd là lane generator (P0-5 — adapter trả Unsupported
        # cho mặt registry; pipeline do người quản sau đó): scaffold-only.)
        "dependency_owner": "scaffold-only",
        "owner_by_operation": {},
    },
]

# Locked v1.2 applicability contract. The generated matrix may report every
# known dimension, but only dimensions declared here are release-required for
# that core/language lane. Keeping this registry in source prevents a JSON
# producer from shrinking `required_dimensions`; it also avoids treating an
# intentionally non-applicable operation (for example `dev` for a library)
# as a failed implementation. Every declared lane must appear exactly once.
LANE_REQUIRED_DIMENSIONS = {
    (lane["core"], lane["language"], lane.get("framework_id", "")): tuple(
        lane["required_dims"]
    )
    for lane in LANES
}


def required_dimensions_for_lane(core, language, framework_id=""):
    """Return the source-controlled release dimensions for one lane."""
    return LANE_REQUIRED_DIMENSIONS.get((core, language, framework_id or ""))

# Per-lane user-operation owners are declarations, not runtime evidence.
# They mirror `DepOp::owner_for` and are compared with `mgc capabilities`
# during the real-binary matrix run. Unprobed operation dimensions remain
# `unverified` below, so this metadata can never promote a lane by itself.
NATIVE_USER_OPERATION_LANES = {
    ("web", "javascript"): set(ALL_DEPENDENCY_OPERATIONS),
    ("web", "vanilla"): set(ALL_DEPENDENCY_OPERATIONS),
    ("web", "ts"): set(ALL_DEPENDENCY_OPERATIONS),
    ("web", "node"): set(ALL_DEPENDENCY_OPERATIONS),
    ("ai", "python"): set(NATIVE_OPERATIONS_EXCEPT_GC_AND_OFFLINE),
    ("app", "flutter"): {"install", "add", "remove", "update"},
    ("app", "swift"): {"install"},
    ("lib", "rust"): {"install", "add", "remove", "update"},
    ("lib", "python"): {"install", "add", "remove", "update", "list"},
    ("lib", "typescript"): set(NATIVE_OPERATIONS_EXCEPT_GC),
    ("lib", "go"): {"install", "add", "remove", "update"},
    ("lib", "java"): {"install", "add", "remove", "update"},
    ("lib", "dotnet"): {"install", "add", "remove", "update"},
    ("game", "rust"): {"install", "add", "remove", "update"},
    ("iot", "rust"): {"install", "add", "remove", "update"},
    ("clo", "terraform"): set(),
    ("clo", "cdk"): set(NATIVE_OPERATIONS_EXCEPT_GC),
    ("clo", "pulumi"): set(NATIVE_OPERATIONS_EXCEPT_GC),
    ("hardware", "benchmark"): set(),
    ("cicd", "github-actions"): set(),
    ("app", "objc"): set(),
    ("app", "react-native"): set(),
}

for _lane in LANES:
    _key = (_lane["core"], _lane["language"])
    if _key not in NATIVE_USER_OPERATION_LANES:
        raise ValueError(f"dependency operation ownership missing for lane {_key}")
    _native_ops = NATIVE_USER_OPERATION_LANES[_key]
    # Offline reinstall is its own capability: Web/TS and the verified
    # Lib/Python wheel-cache lane enforce cache-only installation. Other
    # native online installers still download from registries and must not
    # inherit an offline claim.
    _offline_native = _key in {
        ("web", "javascript"), ("web", "typescript"), ("web", "vanilla"),
        ("web", "ts"), ("web", "node"), ("lib", "typescript"),
        ("lib", "python"), ("clo", "cdk"), ("clo", "pulumi"),
    }
    _lane["dependency_owner"] = (
        "mgc-native" if "install" in _native_ops
        else "scaffold-only" if _key == ("hardware", "benchmark")
        else "unsupported"
    )
    _lane["install_owner"] = (
        "native-engine" if "install" in _native_ops
        else "unsupported"
    )
    _owners = {}
    for _op in ALL_DEPENDENCY_OPERATIONS:
        if _key == ("hardware", "benchmark") and _op == "list":
            _owners[_op] = "scaffold-only"
        elif _op == "offline-reinstall" and _offline_native:
            _owners[_op] = "mgc"
        elif _op in _native_ops or (
            _op in NATIVE_INSTALL_PIPELINE_OPERATIONS and "install" in _native_ops
        ):
            _owners[_op] = "magicore-shared-cas" if _op == "store" else "mgc"
        else:
            _owners[_op] = "unsupported-no-runner"
    _lane["owner_by_operation"] = _owners
    _manifest_variants = {
        ("lib", "rust"): "rust/cargo-toml",
        ("lib", "python"): "python/pep621-native",
        ("lib", "typescript"): "ts/package-json",
        ("lib", "go"): "go/go-mod",
        ("lib", "java"): "java/maven-pom",
        ("lib", "dotnet"): "dotnet/csproj",
        ("ai", "python"): "python/mgc-pyproject",
    }
    if _key in _manifest_variants:
        _lane["manifest_variant"] = _manifest_variants[_key]
del _lane, _key, _native_ops, _owners, _op

# Binary step timeout (seconds) — overridable via MGC_LIFECYCLE_STEP_TIMEOUT.
# Timeout mỗi bước binary — ghi đè qua MGC_LIFECYCLE_STEP_TIMEOUT.
STEP_TIMEOUT_DEFAULT_S = 600

# Schema v4: 23 lifecycle dimensions. Fetching bytes is not proof that MGC
# verified artifact integrity before storing or materializing.
# (Schema v4: tải bytes không chứng minh MGC đã xác minh integrity trước
# khi lưu hoặc materialize.)
# `cache-reuse`/`offline` legacy names are replaced by explicit operations
# `store` and `offline-reinstall` so the
# `.dimensions` list is the canonical 23-name contract — a consumer of the
# JSON can rely on the order and the names below.
# (Schema v4: 23 dimension lifecycle. Tên cũ `cache-reuse`/`offline`
# đổi thành operation rõ `store`/`offline-reinstall` để `.dimensions` là hợp
# đồng 23 tên chuẩn — consumer của JSON dựa được vào tên và thứ tự dưới.)
ALL_DIMENSIONS = ["create", "install", "add", "remove", "update", "list",
                  "test", "build", "run", "dev", "audit", "store",
                  "offline-reinstall", "detect", "resolve", "lock", "fetch",
                  "verify", "materialize", "frozen-install", "gc",
                  "optimizer", "recovery"]

LIFECYCLE_OWNER_VALUES = frozenset(
    {"mgc-native", "managed-delegation", "plain-delegation", "unsupported", "unverified"}
)
LIFECYCLE_STEP_OWNER_DEFAULTS = {
    # The CLI routes these to external project test/build toolchains today.
    # Marking them plain-delegated prevents the wrapper command itself from
    # laundering successful subprocess execution into native evidence.
    "test": "plain-delegation",
    "build": "plain-delegation",
    # `mgc run` starts a project runtime; the runtime remains an external
    # execution dependency even though MGC owns the process boundary.
    "run": "plain-delegation",
}

# A source declaration cannot promote an arbitrary lifecycle subprocess to
# MGC-native. Each exception must be bound to an exact lane and operation
# with a dedicated runtime probe; today only the Web JavaScript dev server
# has that evidence (HTTP/HMR probe). Build/test/run remain delegated until
# an implementation and operation-specific evidence are added together.
# Không được nâng lifecycle subprocess tùy ý thành MGC-native bằng khai báo.
# Mỗi ngoại lệ phải gắn với lane + operation chính xác và probe runtime riêng;
# hiện chỉ Web JavaScript dev server có evidence HTTP/HMR.
NATIVE_LIFECYCLE_OWNER_EVIDENCE = frozenset({
    ("web", "javascript", "", "dev"),
})


def lifecycle_owner_for(lane, dimension):
    """Return the declared owner of a lifecycle dimension, fail-closed."""
    override = lane.get("lifecycle_owner_overrides", {}).get(dimension)
    if override:
        return override
    if dimension == "create":
        return "mgc-native"
    if dimension == "install":
        return {
            "native-engine": "mgc-native",
            "managed-delegation": "managed-delegation",
            "plain-delegation": "plain-delegation",
            "unsupported": "unsupported",
        }.get(lane.get("install_owner", ""), "unverified")
    return LIFECYCLE_STEP_OWNER_DEFAULTS.get(dimension, "unverified")


def lifecycle_pass_status(lane, dimension):
    """Map lifecycle owner to evidence status; unknown ownership never passes."""
    owner = lifecycle_owner_for(lane, dimension)
    return lifecycle_status_for_owner(owner)


def lifecycle_status_for_owner(owner):
    """Map an explicit lifecycle owner to its only valid passing status."""
    if owner == "mgc-native":
        return STATUS_NATIVE
    if owner == "managed-delegation":
        return STATUS_MANAGED
    if owner == "plain-delegation":
        return STATUS_PLAIN
    if owner == "unsupported":
        return STATUS_UNSUPPORTED
    return STATUS_UNVERIFIED


def lifecycle_status_owner_matches(status, owner):
    """Keep a failed probe visible without rewriting it as an owner mismatch.
    (Giữ trạng thái probe thất bại trung thực; không biến nó thành lỗi owner.)"""
    if owner not in LIFECYCLE_OWNER_VALUES:
        return False
    return status == STATUS_FAILED or status == lifecycle_status_for_owner(owner)


def validate_lane_registry(lanes=LANES):
    """Reject ambiguous or internally inconsistent lifecycle lane declarations."""
    lane_keys = set()
    framework_keys = set()
    for lane in lanes:
        core_language = (lane.get("core"), lane.get("language"))
        if not all(core_language):
            raise ValueError(f"lane is missing core/language identity: {lane!r}")
        framework_id = lane.get("framework_id") or ""
        key = (*core_language, framework_id)
        if key in lane_keys:
            raise ValueError(f"duplicate core/language/framework lane: {key}")
        lane_keys.add(key)

        required = lane.get("required_dims")
        if not required or len(required) != len(set(required)):
            raise ValueError(f"lane has empty or duplicate required dimensions: {key}")
        unknown_required = sorted(set(required) - set(ALL_DIMENSIONS))
        if unknown_required:
            raise ValueError(
                f"lane has unknown required dimension(s) {unknown_required}: {key}"
            )
        overrides = lane.get("lifecycle_owner_overrides", {})
        if not isinstance(overrides, dict):
            raise ValueError(f"lane lifecycle_owner_overrides must be an object: {key}")
        invalid_override_dims = sorted(set(overrides) - set(ALL_DIMENSIONS))
        if invalid_override_dims:
            raise ValueError(
                f"lane has unknown lifecycle owner override dimensions "
                f"{invalid_override_dims}: {key}"
            )
        invalid_override_owners = {
            dim: owner for dim, owner in overrides.items()
            if not isinstance(owner, str) or owner not in LIFECYCLE_OWNER_VALUES
        }
        if invalid_override_owners:
            raise ValueError(
                f"lane has invalid lifecycle owner override(s) "
                f"{invalid_override_owners}: {key}"
            )
        unproven_native_overrides = {
            dim: owner
            for dim, owner in overrides.items()
            if owner == "mgc-native"
            and (*key, dim) not in NATIVE_LIFECYCLE_OWNER_EVIDENCE
        }
        if unproven_native_overrides:
            raise ValueError(
                "native lifecycle owner override lacks evidence for exact "
                f"lane/operation {unproven_native_overrides}: {key}"
            )
        invalid_owners = {
            dim: lifecycle_owner_for(lane, dim)
            for dim in required
                if lifecycle_owner_for(lane, dim) not in LIFECYCLE_OWNER_VALUES
        }
        if invalid_owners:
            raise ValueError(f"lane has invalid lifecycle owner(s) {invalid_owners}: {key}")

        step_names = [step[0] for step in lane.get("steps", ())]
        unknown_steps = sorted(set(step_names) - set(ALL_DIMENSIONS))
        if unknown_steps:
            raise ValueError(f"lane has unknown lifecycle step(s) {unknown_steps}: {key}")
        duplicate_steps = sorted(
            name for name in set(step_names) if step_names.count(name) > 1
        )
        if duplicate_steps:
            raise ValueError(f"lane has duplicate lifecycle step(s) {duplicate_steps}: {key}")

        framework_ids = lane.get("framework_ids", ())
        if framework_ids and not framework_id:
            raise ValueError(
                f"lane with framework_ids requires an exact framework_id: {core_language}"
            )
        if framework_id and list(framework_ids) != [framework_id]:
            raise ValueError(
                "framework_id must match the lane's single framework_ids entry: "
                f"{core_language}"
            )
        for framework in framework_ids:
            framework_key = (core_language[0], framework)
            if framework_key in framework_keys:
                raise ValueError(
                    f"duplicate core/framework lifecycle lane: "
                    f"{framework_key[0]}/{framework_key[1]}"
                )
            framework_keys.add(framework_key)
    return True


validate_lane_registry()

# The SIX-VALUE status vocabulary (Gate 11-C). Only the three *-pass
# statuses satisfy a required dimension; `unverified` (skipped/env-missing)
# NEVER auto-promotes to a pass, `unsupported` means the dimension does not
# exist for the lane, `failed` means a crash/step failure.
# (Bộ 6 trạng thái (Gate 11-C). Chỉ ba trạng thái *-pass thỏa dimension
# bắt buộc; `unverified` (skip/thiếu env) KHÔNG BAO GIỜ tự động thành pass,
# `unsupported` = dimension không tồn tại cho lane, `failed` = crash/bước
# fail.)
STATUS_NATIVE = "native-pass"
STATUS_MANAGED = "managed-delegation-pass"
STATUS_PLAIN = "plain-delegation-pass"
STATUS_UNVERIFIED = "unverified"
STATUS_UNSUPPORTED = "unsupported"
STATUS_FAILED = "failed"
PASS_STATUSES = (STATUS_NATIVE, STATUS_MANAGED, STATUS_PLAIN)
ALL_STATUSES = frozenset(
    (*PASS_STATUSES, STATUS_UNVERIFIED, STATUS_UNSUPPORTED, STATUS_FAILED)
)
LIFECYCLE_VERDICTS = frozenset(
    (
        "orchestration-lifecycle-passed",
        "partial",
        "unsupported",
        "evidence-unverified",
    )
)

# JSON schema version v7: framework-qualified lanes, per-dimension lifecycle
# ownership, and the full compiled framework-qualification inventory.
# (Schema JSON v7: lane có framework identity, owner lifecycle từng chiều,
# và inventory qualification framework đầy đủ lấy từ binary đã biên dịch.)
SCHEMA_VERSION = 7

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
FRAMEWORK_QUALIFICATION_EVIDENCE = []


def lifecycle_platform_name() -> str:
    """Return the host OS name recorded in generated lifecycle evidence."""
    return platform.system()


def lifecycle_working_tree_clean() -> bool:
    """Return true only when Git can prove there are no tracked or untracked edits.
    Chỉ trả true khi Git chứng minh không có thay đổi tracked hoặc untracked.
    """
    try:
        result = run_text_capture(
            ["git", "status", "--porcelain", "--untracked-files=all"],
            capture_output=True,
            text=True,
            cwd=REPOSITORY_ROOT,
            check=False,
        )
    except OSError:
        return False
    return result.returncode == 0 and not result.stdout.strip()


def lifecycle_source_matches(commit: str, clean_before_run: bool) -> bool:
    """Require clean source both before and after collection at one commit.
    Bắt buộc source sạch trước/sau khi thu thập và giữ cùng một commit.
    """
    if not clean_before_run or not commit or not lifecycle_working_tree_clean():
        return False
    try:
        result = run_text_capture(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            cwd=REPOSITORY_ROOT,
            check=False,
        )
    except OSError:
        return False
    return result.returncode == 0 and result.stdout.strip() == commit

# P0-D (2026-09-16): per-lane DEPENDENCY OWNER — judged from the ADAPTER
# CODE, never from aspiration. One of exactly four values per lane:
#   mgc-native      mgc itself resolves+fetches+materializes (web engine)
#   delegated       an external toolchain owns the dependency lifecycle
#                   (cargo/uv/go/gradle/dotnet/flutter/pio/west/terraform)
#   scaffold-only   mgc generates the project once; no dependency lifecycle
#                   exists afterwards
#   unsupported     no dependency surface at all
# Verdict gate: `native-pm-supported` ONLY when dependency_owner is
# mgc-native; delegated lanes cap at the NEW `compatibility-passed`
# native-pm verdict; scaffold-only/unsupported read `unsupported`.
# (P0-D: CHỦ SỞ HỮU DEPENDENCY theo lane — phán theo CODE ADAPTER, không
# theo khát vọng. Đúng bốn giá trị mỗi lane: mgc-native / delegated /
# scaffold-only / unsupported. Cổng verdict: `native-pm-supported` CHỈ khi
# mgc-native; lane delegated trần ở verdict native-pm MỚI
# `compatibility-passed`; scaffold-only/unsupported đọc `unsupported`.)
DEPENDENCY_OWNER_VOCABULARY = ("mgc-native", "delegated", "scaffold-only", "unsupported")

# A package-manager ownership claim needs evidence for the dependency engine,
# not just the surrounding create/test/build workflow.
# (Claim sở hữu package manager cần evidence cho dependency engine, không
# chỉ cho workflow create/test/build bao quanh.)
NATIVE_PM_USER_OPERATIONS = (
    "install",
    "add",
    "remove",
    "update",
    "list",
    "frozen-install",
    "offline-reinstall",
    "gc",
)
NATIVE_PM_REQUIRED_DIMENSIONS = (
    *NATIVE_PM_USER_OPERATIONS,
    "resolve",
    "lock",
    "fetch",
    "verify",
    "store",
    "materialize",
)


def native_pm_supported(dependency_owner, install_owner, delegated, dimensions, operation_owners):
    """Require native evidence for each package-engine boundary.
    (Chỉ đạt khi từng ranh giới package-engine có evidence native.)"""
    expected_owners = {
        **{operation: "mgc" for operation in NATIVE_PM_USER_OPERATIONS},
        "resolve": "mgc",
        "lock": "mgc",
        "fetch": "mgc",
        "verify": "mgc",
        "store": "magicore-shared-cas",
        "materialize": "mgc",
    }
    return (
        dependency_owner == "mgc-native"
        and install_owner == "native-engine"
        and not delegated
        and all(dimensions.get(name) == STATUS_NATIVE for name in NATIVE_PM_REQUIRED_DIMENSIONS)
        and all(operation_owners.get(name) == owner for name, owner in expected_owners.items())
    )


def native_pm_unavailable_verdict(dependency_owner):
    """Preserve explicit unsupported ownership when runtime tools are absent.
    (Giữ verdict unsupported đã khai rõ khi thiếu toolchain runtime.)"""
    if dependency_owner in {"scaffold-only", "unsupported"}:
        return "unsupported"
    return "not-native-pm"


def native_pm_claim_scope(lanes):
    """Return every lane that declares MGC-native dependency ownership.
    (Trả mọi lane tự khai MGC sở hữu dependency native.)"""
    return {
        (lane["core"], lane["language"], lane.get("framework_id", ""))
        for lane in lanes
        if lane.get("dependency_owner") == "mgc-native"
    }


def native_pm_claim_errors(lanes):
    """Reject each native ownership claim without complete runtime evidence.
    (Từ chối claim native nếu thiếu bằng chứng runtime đầy đủ.)"""
    failures = []
    for lane in lanes:
        if lane.get("dependency_owner") != "mgc-native":
            continue
        failures.extend(native_pm_lane_errors(lane))
    return failures


def native_pm_lane_errors(lane):
    """Validate native-PM evidence only for native claims; keep other owners explicit.
    (Chỉ đòi native-PM evidence với claim native; owner khác phải khai rõ.)"""
    label = f"{lane.get('core', '?')}/{lane.get('language', '?')}"
    owner = lane.get("dependency_owner")
    install_owner = lane.get("install_owner")
    verdict = lane.get("native_pm_verdict")
    valid_install_owners = {
        "native-engine",
        "managed-delegation",
        "plain-delegation",
        "unsupported",
    }
    if owner not in {"mgc-native", "delegated", "scaffold-only", "unsupported"}:
        return [f"{label} has invalid dependency_owner {owner!r}"]
    if install_owner not in valid_install_owners:
        return [f"{label} has invalid install_owner {install_owner!r}"]
    failures = []
    lifecycle_verdict = lane.get("verdict")
    if (
        not isinstance(lifecycle_verdict, str)
        or lifecycle_verdict not in LIFECYCLE_VERDICTS
    ):
        failures.append(f"{label} has invalid lifecycle verdict {lifecycle_verdict!r}")
    if owner == "mgc-native":
        if install_owner != "native-engine":
            failures.append(
                f"{label} declares mgc-native with install_owner {install_owner!r}"
            )
        dimensions = lane.get("dimensions")
        if not isinstance(dimensions, dict):
            failures.append(f"{label} has malformed native-PM dimensions")
            dimensions = {}
        delegated = lane.get("native_pm_delegated")
        if not isinstance(delegated, list):
            failures.append(f"{label} has malformed native-PM delegation evidence")
            delegated = ["invalid"]
        operation_owners = lane.get("owner_by_operation")
        if not isinstance(operation_owners, dict):
            failures.append(f"{label} has malformed native-PM operation ownership")
            operation_owners = {}
        if verdict == "native-pm-supported" and not native_pm_supported(
            owner,
            install_owner,
            delegated,
            dimensions,
            operation_owners,
        ):
            failures.append(
                f"{label} declares native-pm-supported without complete package-manager evidence"
            )
        elif verdict not in {"native-pm-supported", "not-native-pm"}:
            failures.append(
                f"{label} mgc-native owner requires native_pm_verdict 'native-pm-supported' or 'not-native-pm', got {verdict!r}"
            )
        return failures
    if owner == "delegated":
        if install_owner not in {"managed-delegation", "plain-delegation"}:
            failures.append(
                f"{label} delegated owner has incompatible install_owner {install_owner!r}"
            )
        expected = (
            "compatibility-passed"
            if lifecycle_verdict == "orchestration-lifecycle-passed"
            else "not-native-pm"
        )
        if verdict != expected:
            failures.append(
                f"{label} delegated owner requires native_pm_verdict {expected!r}, got {verdict!r}"
            )
        return failures
    release_reason = lane.get("release_scope_out_reason")
    if not isinstance(release_reason, str) or not release_reason.strip():
        failures.append(
            f"{label} {owner} owner is missing an explicit release scope-out reason"
        )
    if install_owner != "unsupported" or verdict != "unsupported":
        failures.append(
            f"{label} {owner} owner requires unsupported install and native-PM verdicts"
        )
    return failures


def lifecycle_lane_owner_contract_errors(lane):
    """Require matrix package owners to match the source lane registry.
    (Bắt owner package trong matrix khớp registry lane trong source.)"""
    core = lane.get("core", "?")
    language = lane.get("language", "?")
    framework_id = lane.get("framework_id", "") or ""
    label = f"{core}/{language}" + (f"#{framework_id}" if framework_id else "")
    source_lane = next(
        (
            candidate for candidate in LANES
            if candidate["core"] == core
            and candidate["language"] == language
            and (candidate.get("framework_id", "") or "") == framework_id
        ),
        None,
    )
    if source_lane is None:
        return [f"{label} has no source owner contract"]
    errors = []
    for field, description in (
        ("dependency_owner", "dependency owner"),
        ("install_owner", "install owner"),
    ):
        actual = lane.get(field)
        expected = source_lane.get(field)
        if actual != expected:
            errors.append(
                f"{label} {description} differs from source contract: "
                f"matrix={actual!r}, source={expected!r}"
            )
    matrix_operations = lane.get("owner_by_operation")
    source_operations = source_lane.get("owner_by_operation")
    if not isinstance(matrix_operations, dict):
        errors.append(f"{label} is missing per-operation ownership")
    elif not isinstance(source_operations, dict):
        errors.append(f"{label} source contract is missing per-operation ownership")
    else:
        for operation in ALL_DEPENDENCY_OPERATIONS:
            actual = matrix_operations.get(operation)
            expected = source_operations.get(operation)
            if actual != expected:
                errors.append(
                    f"{label} operation owner differs from source contract for {operation}: "
                    f"matrix={actual!r}, source={expected!r}"
                )
        extra_operations = sorted(set(matrix_operations) - set(ALL_DEPENDENCY_OPERATIONS))
        if extra_operations:
            errors.append(
                f"{label} has unknown package operation owners: "
                + ", ".join(extra_operations)
            )
    return errors


def evidence_commit_error(
    checkout_sha,
    expected_sha,
    *,
    is_ci=False,
    recorded_sha=None,
    require_recorded=False,
):
    """Reject CI evidence whose checked-out source differs from the run SHA."""
    if not checkout_sha:
        return "matrix checkout did not resolve a commit SHA"
    if is_ci and not expected_sha:
        return "GITHUB_SHA is required when generating lifecycle evidence in CI"
    if expected_sha and checkout_sha.lower() != expected_sha.lower():
        return f"matrix checkout SHA {checkout_sha} does not match workflow SHA {expected_sha}"
    if is_ci and require_recorded:
        if not recorded_sha:
            return "MGC_PLATFORM_EVIDENCE_SHA is required when recording CI green evidence"
        if recorded_sha.lower() != expected_sha.lower():
            return "platform evidence SHA does not match workflow GITHUB_SHA"
    return None

# ---------------------------------------------------------------------------
# P0-3 platform-evidence counter (Tech Lead 2026-09-16): windows-latest
# runs the SAME lifecycle matrix but is evidence-only — and "experimental"
# must never be permanent. The gitignored counter file (same treatment as
# the matrix JSON) records CONSECUTIVE fully-green matrix runs per OS:
#   {"<os>": {"runs": N, "last_green_sha": "...", "last_green_at": "..."}}
# The CI workflow increments it after every green Windows run
# (--record-green), zeroes it whenever the Windows lane breaks (--reset),
# and the windows-evidence-promotion job fails the workflow once the count
# reaches PLATFORM_EVIDENCE_PROMOTION_THRESHOLD — the self-enforcing
# "flip Windows to required now" tripwire.
# (── Counter evidence platform P0-3 (Tech Lead 2026-09-16):
# windows-latest chạy CÙNG lifecycle matrix nhưng chỉ thu evidence — và
# "experimental" không được thành vĩnh viễn. File counter gitignored (xử
# lý như matrix JSON) ghi số lần chạy matrix XANH TRỌN VẸN LIÊN TIẾP theo
# OS: {"<os>": {"runs": N, "last_green_sha": "...", "last_green_at":
# "..."}}. Workflow CI tăng sau mỗi lần Windows xanh (--record-green), về
# 0 khi lane Windows vỡ (--reset), và job windows-evidence-promotion FAIL
# workflow khi đủ PLATFORM_EVIDENCE_PROMOTION_THRESHOLD — bẫy tự-ép "đổi
# Windows thành required ngay".)
PLATFORM_EVIDENCE_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    os.pardir, "docs", "specs", "lifecyclePlatformEvidence.json",
)
PLATFORM_EVIDENCE_PROMOTION_THRESHOLD = 3
PLATFORM_EVIDENCE_OS_NAMES = {
    "windows-latest": "Windows",
    "macos-latest": "Darwin",
    "ubuntu-latest": "Linux",
}

# V1.2 promotion scope is the complete declared platform, not the old six-lane
# RC subset. Adding a lane therefore expands the gate automatically.
# (Phạm vi promotion V1.2 là toàn nền tảng đã khai báo, không phải sáu lane
# RC cũ. Thêm lane tự động mở rộng gate.)
PLATFORM_EVIDENCE_RELEASE_SCOPE = frozenset(
    (lane["core"], lane["language"], lane.get("framework_id", ""))
    for lane in LANES
)

# Keep lifecycle evidence-only lanes in the inventory with an exact reason.
# A scope-out never removes the lane or its package claim; native claims still
# pass the separate completeness gate for every required dimension/operation.
# (Giữ lifecycle evidence-only lanes trong inventory cùng lý do chính xác.
# Scope-out không xóa lane hay claim package; claim native vẫn phải qua gate
# riêng kiểm đủ dimension và operation.)
LANE_RELEASE_SCOPE_OUT_REASONS = {
    ("lib", "java", "java"): (
        "Java dependency resolution exists, but native Java test/build execution is not implemented or release-qualified."
    ),
    ("lib", "dotnet", "dotnet"): (
        "Native NuGet install exists, but MGC does not produce the MSBuild restore state required by `dotnet test/build --no-restore`; .NET test/build is outside the approved 00-index §5.1 tool allowlist for this release."
    ),
    ("app", "swift", ""): (
        "Swift lifecycle evidence remains non-blocking until release E2E qualifies app lifecycle parity."
    ),
    ("app", "objc", ""): (
        "Objective-C dependency lifecycle is unsupported and not qualified for v1.2 release."
    ),
    ("app", "react-native", ""): (
        "React Native has scaffold evidence only; native dependency lifecycle is not qualified for v1.2."
    ),
    ("clo", "terraform", ""): (
        "Terraform dependency operations are unsupported; the declared cloud release lanes are CDK and Pulumi."
    ),
    ("game", "rust", ""): (
        "Game/Rust lifecycle evidence remains non-blocking until the game lane is qualified for release."
    ),
    ("hardware", "benchmark", ""): (
        "Hardware is generator and benchmark tooling only; it has no package lifecycle."
    ),
    ("iot", "rust", ""): (
        "IoT/Rust lifecycle evidence remains non-blocking until board toolchains and the lane are release-qualified."
    ),
    ("cicd", "github-actions", ""): (
        "GitHub Actions is a workflow scaffold; install, test, and build run on the external CI provider."
    ),
}


def release_scope_out_reason(core, language, framework_id=""):
    """Return the source-controlled reason a lane is visible but out of release scope.
    (Trả lý do từ source cho lane vẫn hiện nhưng nằm ngoài phạm vi release.)"""
    return LANE_RELEASE_SCOPE_OUT_REASONS.get((core, language, framework_id or ""))


# Manifest markers per language: the file that PROVES the scaffold
# actually produced a project of the requested language (not a
# fallback template of another language wearing the label). A scaffold
# that emits the wrong marker is a FAILED create — honest, never passed.
# Marker theo ngôn ngữ: file CHỨNG MINH scaffold sinh đúng project của
# ngôn ngữ yêu cầu (không phải template fallback ngôn ngữ khác gắn
# nhãn). Scaffold sinh sai marker → create FAILED (trung thực).
SCAFFOLD_MARKERS = {
    "rust": ["Cargo.toml"],
    "python": ["pyproject.toml", "requirements.txt"],
    "typescript": ["package.json", "tsconfig.json"],
    "go": ["go.mod"],
    "java": ["pom.xml", "build.gradle", "build.gradle.kts"],
    "dotnet": ["*.csproj", "*.sln"],
    "javascript": ["package.json"],
    "vanilla": ["index.html"],
    "ts": ["index.html"],
    "node": ["package.json"],
    "cdk": ["package.json"],
    "pulumi": ["package.json", "Pulumi.yaml"],
    "flutter": ["pubspec.yaml"],
    # Evidence-only lane markers (P0-6): the file that PROVES the scaffold
    # produced the right artifact for these lanes.
    # (Marker lane evidence-only (P0-6): file CHỨNG MINH scaffold sinh đúng
    # artifact cho các lane này.)
    "terraform": ["main.tf"],
    "github-actions": [".github/workflows/ci.yml"],
    # `benchmark` (hardware lane) has no manifest file — the language falls
    # into the "cannot verify, do not guess" path of
    # scaffold_language_matches.
    # (`benchmark` (lane hardware) không có file manifest — ngôn ngữ này rơi
    # vào nhánh "không verify được, không đoán" của scaffold_language_matches.)
}


def scaffold_language_matches(
    sandbox: str, project_dir: str, language: str, required_markers=None
) -> bool:
    """Verify the scaffolded project carries a manifest of the REQUESTED
    language (anti-fake-scaffold guard, P0-2 honesty contract).
    Xác minh project được sinh có manifest của ĐÚNG ngôn ngữ yêu cầu
    (chống scaffold giả — hợp đồng trung thực P0-2)."""
    markers = SCAFFOLD_MARKERS.get(language, [])
    if not markers:
        return True  # unknown language — cannot verify, do not guess
    root = os.path.join(sandbox, project_dir)
    if required_markers:
        return all(os.path.isfile(os.path.join(root, marker)) for marker in required_markers)
    for marker in markers:
        if marker.startswith("*"):
            # Glob suffix (e.g. *.csproj) — any file matching counts.
            # Hậu tố glob (vd *.csproj) — bất kỳ file khớp đều tính.
            import glob as _glob
            if _glob.glob(os.path.join(root, marker)):
                return True
        elif os.path.isfile(os.path.join(root, marker)):
            return True
    return False


# ---------------------------------------------------------------------------
# Gate 11-C fine-grained evidence helpers (schema v7 dimensions: detect,
# resolve, lock, fetch, materialize, optimizer, recovery).
# (Hàm phụ trợ bằng chứng chi tiết Gate 11-C cho các dimension schema v7.)
# ---------------------------------------------------------------------------


def _owner_pass_status(lane: dict) -> str:
    """Map the lane's DECLARED install_owner to the pass status recorded on
    owner-covered dimensions (install + delegated steps + the resolve/lock/
    fetch/materialize evidence). An mgc-orchestrated success in a managed-
    delegation lane stays managed — a pass can never read as more native
    than the taxonomy declares.
    (Map install_owner ĐÃ KHAI của lane sang trạng thái pass cho các
    dimension thuộc quyền owner (install + bước delegated + bằng chứng
    resolve/lock/fetch/materialize). Thành công do mgc điều phối trong lane
    managed-delegation vẫn là managed — pass không bao giờ được đọc là
    native hơn taxonomy khai.)"""
    return {
        "native-engine": STATUS_NATIVE,
        "managed-delegation": STATUS_MANAGED,
        "plain-delegation": STATUS_PLAIN,
        "unsupported": STATUS_UNSUPPORTED,
    }.get(lane.get("install_owner", ""), STATUS_PLAIN)


def _dir_nonempty(path: str) -> bool:
    """True when `path` is a directory containing at least one entry.
    (True khi `path` là thư mục có ít nhất một mục con.)"""
    try:
        with os.scandir(path) as entries:
            return any(entries)
    except OSError:
        return False


def _lockfile_candidates(project_dir: str, language: str) -> list:
    """Lockfile artifacts that can prove resolve+lock per language: a list
    of (name, kind) probes in evidence-preference order. kind is
    'mgc-json' (mgc.lock — top-level "package" list), 'toml-packages'
    ([[package]] sections), 'text-lines' (non-comment lines), 'pubspec'
    (pubspec.lock `packages:` section) or 'json-deps' (a dependencies
    object). An empty list means no known artifact — the lock/resolve
    dimensions stay `unverified` for that language, never a guessed pass.
    (Các artifact lockfile có thể chứng minh resolve+lock theo ngôn ngữ:
    danh sách (tên, kiểu) theo thứ tự ưu tiên bằng chứng. Danh sách rỗng =
    không có artifact nào biết — dimension lock/resolve giữ `unverified`
    cho ngôn ngữ đó, không bao giờ đoán pass.)"""
    if language in ("javascript", "typescript", "pulumi"):
        # mgc.lock is written as TOML `[[package]]` on disk (see
        # core/crates/mgc-lockfile/src/writer.rs — toml::to_string_pretty);
        # the JSON spelling is a legacy fallback probe, never a guess.
        # (mgc.lock ghi ra đĩa là TOML `[[package]]`; JSON là probe dự
        # phòng cho legacy, không phải đoán.)
        return [("mgc.lock", "mgc-lock"), ("package-lock.json", "json-deps")]
    if language == "rust":
        # mgc.lock carries the mgc-owned lock claim
        # (owner_by_operation.lock); Cargo.lock is cargo's own — both are
        # lock evidence, and the per-operation owner taxonomy names who
        # owns what.
        # (mgc.lock mang claim lock của mgc (owner_by_operation.lock);
        # Cargo.lock là của cargo — cả hai đều là bằng chứng lock, taxonomy
        # owner per-operation nêu rõ ai giữ gì.)
        return [("mgc.lock", "mgc-lock"), ("Cargo.lock", "toml-packages")]
    if language == "python":
        return [("uv.lock", "toml-packages")]
    if language == "go":
        return [("go.sum", "text-lines")]
    if language == "flutter":
        return [("pubspec.lock", "pubspec")]
    if language == "java":
        return [("gradle.lockfile", "text-lines")]
    if language == "dotnet":
        return [("packages.lock.json", "json-deps")]
    return []


def _lockfile_entry_count(path: str, kind: str):
    """Best-effort entry count for a lockfile probe; None when the content
    cannot be interpreted — which records `unverified`, never a pass.
    (Đếm mục lockfile theo kiểu probe; None khi không đọc hiểu được nội
    dung — khi đó ghi `unverified`, không bao giờ pass.)"""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()
    except OSError:
        return None
    if kind == "mgc-lock":
        # On-disk mgc.lock is TOML with one `[[package]]` table per
        # resolved package; some legacy writers may emit JSON with a
        # top-level "package" list — accept either, count what parses.
        # (mgc.lock trên đĩa là TOML với một bảng `[[package]]` mỗi
        # package; writer legacy có thể ghi JSON với list "package" —
        # nhận cả hai, đếm đúng cái parse được.)
        toml_count = _lockfile_entry_count(path, "toml-packages")
        if toml_count:
            return toml_count
        json_count = _lockfile_entry_count(path, "mgc-json")
        return json_count
    if kind == "mgc-json":
        try:
            data = json.loads(content)
        except ValueError:
            return None
        pkgs = data.get("package") if isinstance(data, dict) else None
        return len(pkgs) if isinstance(pkgs, list) else None
    if kind == "toml-packages":
        # Cargo.lock / uv.lock carry one `[[package]]` array entry per
        # resolved package — a structural count that needs no TOML parser
        # (older Pythons have no stdlib tomllib; the regex marker is
        # stable across both lockfile formats).
        # (Cargo.lock / uv.lock có một mục `[[package]]` cho mỗi package
        # đã resolve — đếm cấu trúc không cần parser TOML; marker regex
        # ổn định trên cả hai format lockfile.)
        return len(re.findall(r"^\s*\[\[package\]\]", content, re.M))
    if kind == "text-lines":
        lines = [
            ln for ln in content.splitlines()
            if ln.strip() and not ln.strip().startswith("#")
        ]
        return len(lines) if lines else 0
    if kind == "pubspec":
        # pubspec.lock: a `packages:` section with one `  name:` line per
        # entry (two-space indent); nested keys indent deeper.
        # (pubspec.lock: mục `packages:` với một dòng `  tên:` mỗi entry
        # (thụt 2 dấu cách); khóa lồng nhau thụt sâu hơn.)"""
        if "packages:" not in content:
            return 0
        return len(re.findall(r"^ {2}[^ ].*:$", content, re.M))
    if kind == "json-deps":
        try:
            data = json.loads(content)
        except ValueError:
            return None
        if not isinstance(data, dict):
            return None
        deps = data.get("dependencies")
        if isinstance(deps, dict):
            return len(deps)
        if isinstance(deps, list):
            return len(deps)
        return None
    return None


def _materialize_marker(
    project_dir: str, language: str, environment: Optional[dict[str, str]] = None
):
    """Return (label, path) whose non-emptiness PROVES the dependency tree
    landed after install, or (None, None) when this ecosystem has no
    observable marker — which records `unverified`, never a pass.
    (Trả về (nhãn, đường_dẫn) mà việc không-rỗng CHỨNG MINH cây dependency
    đã thật sự về sau install, hoặc (None, None) khi ecosystem không có
    marker quan sát được — ghi `unverified`, không bao giờ pass.)"""
    if language in ("javascript", "typescript", "pulumi"):
        return ("node_modules", os.path.join(project_dir, "node_modules"))
    if language == "python":
        return (".venv", os.path.join(project_dir, ".venv"))
    if language == "rust":
        # Rust deps never land in-project — cargo materializes them into
        # the MGC-managed global store ($HOME/.magicore/store/cargo, see
        # adapters/lib/src/install/shared_store.rs). That managed store is
        # the materialization surface for this lane.
        # (Deps Rust không bao giờ về trong project — cargo materialize
        # vào CARGO_HOME do mgc quản (~/.magicore/store/cargo). Store quản
        # lý đó là mặt materialize của lane này.)
        home = (environment or os.environ).get("HOME") or os.path.expanduser("~")
        return ("mgc-cargo-store", os.path.join(home, ".magicore", "store", "cargo"))
    if language == "go":
        # The go module cache lives at GOMODCACHE — observable only when
        # the go toolchain is on PATH; otherwise unverified.
        # (Module cache của go nằm ở GOMODCACHE — chỉ quan sát được khi go
        # có trên PATH; không thì unverified.)
        env = environment or os.environ
        go = shutil.which("go", path=env.get("PATH"))
        if go is None:
            return (None, None)
        try:
            proc = run_text_capture(
                [go, "env", "GOMODCACHE"], capture_output=True, text=True,
                timeout=30, env=env,
            )
        except (OSError, subprocess.TimeoutExpired):
            return (None, None)
        cache = (proc.stdout or "").strip()
        return ("gomodcache", cache) if cache else (None, None)
    if language == "flutter":
        return (".dart_tool", os.path.join(project_dir, ".dart_tool"))
    # java/dotnet: no reliable post-install marker without running the
    # toolchain build — honestly unobservable here.
    # (java/dotnet: không có marker đáng tin sau install nếu không chạy
    # build toolchain — trung thực là không quan sát được ở đây.)
    return (None, None)


# The recovery probe crashes `mgc install` at this failpoint phase (the
# FIRST phase of the web install orchestrator's generation lifecycle —
# see core/crates/mgc-store/src/failpoint.rs KNOWN_PHASES).
# (Probe recovery crash `mgc install` ở phase failpoint này — phase ĐẦU
# TIÊN của vòng đời generation trong orchestrator install web.)
RECOVERY_PHASE = "after-generation-begin"
RECOVERY_READY_TIMEOUT_S = 60


def _recovery_probe(mgc_bin: str, project_path: str, sandbox: str,
                    timeout_s: int, detail: dict) -> bool:
    """THE recovery dimension's evidence: run `mgc install` with the
    built-in failpoint handshake — the binary parks at after-generation-
    begin after fsync-ing a READY marker, the harness SIGKILLs it, then
    `mgc store doctor --repair` must leave the store HEALTHY with 0 stale
    staging. Mirrors the Rust contract in cli/tests/kill_injection_matrix.rs.
    Returns True only on full pass; every violation returns False with the
    evidence tail in detail['recovery_output'] — never a guessed pass.
    (Bằng chứng dimension recovery: chạy `mgc install` với handshake
    failpoint có sẵn — binary đỗ tại after-generation-begin sau khi fsync
    marker READY, harness SIGKILL, rồi `mgc store doctor --repair` phải để
    store HEALTHY với 0 stale staging. Khuôn theo hợp đồng Rust trong
    cli/tests/kill_injection_matrix.rs. Chỉ trả True khi pass trọn vẹn;
    mọi vi phạm trả False kèm đuôi bằng chứng trong detail — không bao giờ
    đoán pass.)"""
    import signal

    # Fresh-project rule (mirrors cli/tests/kill_injection_matrix.rs: one
    # FRESH project per probe): the scenario runs on a COPY of the
    # scaffolded project WITHOUT node_modules/lockfile/store state, so the
    # crash-recovery evidence can never be masked by unrelated state from
    # the lane's earlier install (e.g. a stale lockfile tripping the
    # downgrade guard). Every install below is still a REAL install; no
    # security guard is weakened or bypassed.
    # (Luật project-mới (khuôn kill_injection_matrix.rs: mỗi probe một
    # project MỚI): kịch bản chạy trên BẢN COPY của project scaffold
    # KHÔNG kèm node_modules/lockfile/store, để bằng chứng crash-recovery
    # không bao giờ bị trạng thái cũ từ install trước đó của lane che
    # mất (vd lockfile cũ đụng downgrade guard). Mọi install bên dưới vẫn
    # là install THẬT; không guard bảo mật nào bị làm yếu hay bypass.)
    rec_project = os.path.join(sandbox, "recovery-project")
    shutil.copytree(
        project_path, rec_project,
        ignore=shutil.ignore_patterns(
            "node_modules", ".magicore*", ".venv", "target", "dist", "build"
        ),
    )

    # Isolated per-project store (the kill-matrix pattern): the whole
    # crash+repair scenario runs against ITS OWN store, never the user's.
    # (Store per-project cô lập (khuôn kill-matrix): toàn kịch bản
    # crash+repair chạy trên store RIÊNG, không bao giờ store của user.)
    env = recovery_environment(sandbox, rec_project)

    def _doctor(*extra: str):
        proc = run_text_capture(
            [mgc_bin, "store", "doctor", "--core", "web", *extra],
            capture_output=True, text=True, cwd=rec_project, env=env,
            timeout=min(timeout_s, 120),
        )
        return proc.returncode, (proc.stdout or "") + (proc.stderr or "")

    # Baseline: one healthy install populates the isolated store so the
    # victim crashes against a real (non-empty) store.
    # (Baseline: một install khỏe làm đầy store cô lập để victim crash
    # trên store thật (không rỗng).)
    try:
        baseline = run_text_capture(
            [mgc_bin, "install"], capture_output=True, text=True,
            cwd=rec_project, env=env, timeout=timeout_s,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        detail["recovery_output"] = f"baseline install could not run: {exc}"
        return False
    if baseline.returncode != 0:
        detail["recovery_output"] = (
            "baseline install failed: "
            + (baseline.stdout or "") + (baseline.stderr or "")
        )[-2000:]
        return False

    # Victim: spawn the re-install parked at the failpoint; poll the
    # fsync'd READY marker (no guessed sleeps); SIGKILL; assert it truly
    # died by signal.
    # (Victim: spawn install-lại đỗ tại failpoint; poll marker READY đã
    # fsync (không đoán trễ); SIGKILL; khẳng định chết THẬT vì signal.)
    marker = os.path.join(sandbox, "recovery-ready.marker")
    if os.path.exists(marker):
        os.remove(marker)
    victim_err_path = os.path.join(sandbox, "recovery-victim.err")
    victim_env = dict(env)
    victim_env["MGC_FAILPOINT"] = RECOVERY_PHASE
    victim_env["MGC_FAILPOINT_READY_FILE"] = marker
    with open(os.path.join(sandbox, "recovery-victim.out"), "w") as out_f, \
            open(victim_err_path, "w") as err_f:
        try:
            victim = subprocess.Popen(
                [mgc_bin, "install"], cwd=rec_project, env=victim_env,
                stdout=out_f, stderr=err_f, start_new_session=True,
            )
        except OSError as exc:
            detail["recovery_output"] = f"victim spawn failed: {exc}"
            return False
        ready = False
        deadline = time.monotonic() + RECOVERY_READY_TIMEOUT_S
        try:
            while time.monotonic() < deadline:
                try:
                    with open(marker, "r", encoding="utf-8", errors="replace") as f:
                        if f"READY:{RECOVERY_PHASE}" in f.read():
                            ready = True
                            break
                except OSError:
                    pass
                if victim.poll() is not None:
                    break  # died/exited before parking — never a pass
                time.sleep(0.1)
            if not ready:
                if victim.poll() is None:
                    victim.kill()
                    try:
                        victim.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        pass
                return False
            os.kill(victim.pid, signal.SIGKILL)
            try:
                victim.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
        except Exception:
            if victim.poll() is None:
                victim.kill()
            raise
    if victim.returncode is None or victim.returncode >= 0:
        try:
            with open(victim_err_path, "r", encoding="utf-8", errors="replace") as f:
                tail = f.read()[-800:]
        except OSError:
            tail = ""
        detail["recovery_output"] = (
            f"victim did not die by SIGKILL (rc={victim.returncode}); stderr tail: {tail}"
        )
        return False

    # Post-kill gates (the kill-matrix contract): no orphan claims, no lost
    # or corrupt blobs. Stale staging IS expected here — the doctor's exit
    # code at this point is ignored; the strings are the evidence.
    # (Cổng sau kill (hợp đồng kill-matrix): không claim mồ côi, không mất/
    # hỏng blob. Staging stale LÀ điều kỳ vọng — exit code doctor ở điểm
    # này bỏ qua; chuỗi output mới là bằng chứng.)
    rc, out = _doctor()
    gates = ("0 orphan claim(s)", "0 missing blob(s)", "0 corrupt blob(s)")
    if not all(g in out for g in gates):
        detail["recovery_output"] = ("post-kill doctor gates violated: " + out)[-2000:]
        return False

    # Repair must succeed, and the FINAL doctor must be HEALTHY with 0
    # stale staging (the P0-B exit contract is trusted here).
    # (Repair phải thành công, và doctor CUỐI phải HEALTHY với 0 stale
    # staging (hợp đồng exit P0-B được tin ở đây).)
    rc, out = _doctor("--repair")
    if rc != 0:
        detail["recovery_output"] = ("doctor --repair failed: " + out)[-2000:]
        return False
    rc, out = _doctor()
    if rc != 0 or "0 stale staging gen(s)" not in out or "HEALTHY" not in out:
        detail["recovery_output"] = ("final doctor not HEALTHY/0-stale: " + out)[-2000:]
        return False
    detail["recovery_output"] = (
        f"SIGKILL at {RECOVERY_PHASE} → doctor gates clean → --repair ok → "
        "final doctor HEALTHY with 0 stale staging"
    )
    return True


def _fail(message: str, detail: str = "") -> None:
    """Abort generation — a crashed binary run must never look like a
    capability matrix (fail-closed collection, same contract as audit).
    Huỷ sinh matrix — binary crash không bao giờ được thành matrix."""
    print(f"LIFECYCLE MATRIX COLLECTION FAILED: {message}", file=sys.stderr)
    if detail:
        print(detail[-4000:], file=sys.stderr)
    sys.exit(1)


# Operations whose per-operation owner every lane MUST declare (Gate
# 11-C, vòng-11): one install_owner label per lane is NOT enough to
# describe an ecosystem — the audit demands the capability split.
# (Các operation mà mỗi lane PHẢI khai báo owner riêng (Gate 11-C): một
# nhãn install_owner mỗi lane KHÔNG đủ để mô tả ecosystem — audit đòi
# tách năng lực.)
REQUIRED_OWNER_OPERATIONS = list(ALL_DEPENDENCY_OPERATIONS)


def validate_lane_owners() -> int:
    """Gate 11-C owner gate: every lane declares install_owner AND an
    owner for each install operation including integrity verification, and the per-operation owners agree
    with the coarse install_owner label (native-engine lanes must have
    mgc/magicore owning resolve, lock, fetch, verify, store, materialize — any
    native-toolchain owner in that set contradicts the label and BLOCKS).
    Trả về 0 khi sạch; in lỗi và trả 1 khi có lane vi phạm.
    Cổng owner Gate 11-C: mọi lane khai install_owner VÀ owner cho từng
    operation install và xác minh integrity; owner per-operation phải khớp nhãn install_owner
    thô (lane native-engine phải có mgc/magicore giữ resolve, lock,
    fetch, verify, store, materialize — owner toolchain-native trong tập đó mâu
    thuẫn với nhãn và BỊ CHẶN)."""
    violations = []
    for lane in LANES:
        tag = f"{lane['core']}/{lane['language']}"
        owner = lane.get("install_owner")
        if not owner:
            violations.append(f"{tag}: missing install_owner")
            continue
        if owner not in ("native-engine", "managed-delegation", "plain-delegation", "unsupported"):
            violations.append(f"{tag}: unknown install_owner '{owner}'")
        ops = lane.get("owner_by_operation", {})
        if not ops:
            violations.append(f"{tag}: missing owner_by_operation (Gate 11-C per-operation taxonomy)")
            continue
        for op in REQUIRED_OWNER_OPERATIONS:
            if not ops.get(op):
                violations.append(f"{tag}: owner_by_operation missing '{op}'")
        # The coarse install label describes only Install. Each other
        # operation is checked independently against the compiled binary;
        # it must not inherit a native claim from Install.
        if owner == "native-engine":
            install_owner = ops.get("install", "")
            if not (install_owner.startswith("mgc") or install_owner.startswith("magicore")):
                violations.append(f"{tag}: install_owner=native-engine but install is not MGC-owned")
        elif owner == "unsupported" and not ops.get("install", "").startswith("unsupported"):
            violations.append(f"{tag}: install_owner=unsupported but install is not unsupported")
        # A delegation label must show the toolchain actually owning the
        # operations (at least fetch) — otherwise the delegation label is
        # unverified.
        # (Nhãn ủy quyền phải cho thấy toolchain thật sự giữ các
        # operation (ít nhất fetch) — nếu không nhãn ủy quyền chưa được
        # kiểm.)
        if owner in ("managed-delegation", "plain-delegation"):
            fetch_owner = ops.get("fetch", "")
            if fetch_owner.startswith("mgc") or fetch_owner.startswith("magicore"):
                violations.append(
                    f"{tag}: install_owner={owner} but fetch is owned by mgc — "
                    f"mislabeled (should be native-engine or the ops are wrong)"
                )
    if violations:
        for v in violations:
            print(f"OWNER GATE VIOLATION: {v}", file=sys.stderr)
        return 1
    print(f"owner gate: {len(LANES)} lanes clean (install_owner + per-operation owners consistent)")
    return 0


# ---------------------------------------------------------------------------
# P0-5 adapter-capability ground truth (Tech Lead verdict 2026-09-16):
# hardcoded from the PRODUCTION adapters — what each core's PackageAdapter
# actually returns for the registry lifecycle surface. The lane taxonomy
# must match this table, never the aspiration (fail-closed gate below).
# Evidence per row (dẫn chứng từng dòng):
#   web   → TRUE×3: the native engine IS the registry pipeline
#           (impl PackageAdapter at adapters/web/src/lib.rs:265, install/
#           orchestrator + CAS + mgc.lock).
#   lib   → TRUE×3: TRUE for the TS lane — LibAdapter delegates resolve/
#           fetch to the embedded web engine (adapters/lib/src/adapter.rs).
#           P0-B (2026-09-16): resolve/fetch now FAIL CLOSED for the
#           toolchain-owned languages (rust/python/go/java/dotnet) — the
#           core-level row stays TRUE because the TS lane's mgc-owned ops
#           are real; delegated lib lanes never claim mgc owners.
#   ai    → TRUE×3: implemented fail-closed (requires uv.lock /
#           requirements.lock) — adapters/ai/src/adapter.rs:57/61/65.
#   app   → TRUE×3: implemented (flutter pub orchestration) —
#           adapters/app/src/adapter.rs:52/56/60.
#   game  → adapter protocol lacks generic resolve/fetch, but the Bevy CLI
#           route invokes the Lib/Rust native engine (install/game.rs).
#   iot   → adapter protocol lacks generic resolve/fetch, but esp32-rust
#           CLI route invokes the Lib/Rust native engine (install/iot.rs).
#   clo   → resolve/fetch FALSE outside the embedded web engine —
#           terraform/CDK modules are fetched by `terraform init`
#           (adapters/cloud/src/adapter.rs:78/93); install TRUE
#           (adapter.rs:106).
#   hardware → FALSE×3: MgError::Unsupported for resolve/fetch/install
#           (adapters/hardware/src/adapter.rs:55/67/77), plus
#           write_manifest (:41) and update (:111).
#   cicd  → FALSE×3: MgError::Unsupported for resolve/fetch/install
#           (adapters/cicd/src/adapter.rs:65/75/83), plus write_manifest
#           (:52); add/remove/update → "cicd has no package manager".
# (Bảng năng lực adapter theo core; CLI route override phải được nêu đích
# danh vì một adapter Unsupported không phủ định một route MGC khác.)
# ---------------------------------------------------------------------------
ADAPTER_REGISTRY_CAPABILITIES = {
    "web":      {"resolve": True,  "fetch": True,  "install": True},
    "lib":      {"resolve": True,  "fetch": True,  "install": True},
    "ai":       {"resolve": True,  "fetch": True,  "install": True},
    "app":      {"resolve": True,  "fetch": True,  "install": True},
    "game":     {"resolve": False, "fetch": False, "install": True},
    "iot":      {"resolve": False, "fetch": False, "install": True},
    "clo":      {"resolve": False, "fetch": False, "install": True},
    "hardware": {"resolve": False, "fetch": False, "install": False},
    "cicd":     {"resolve": False, "fetch": False, "install": False},
}

# Some CLI dependency lanes intentionally route through the shared Lib
# resolver instead of their core adapter. Keep these explicit so a core
# adapter's unsupported package trait cannot erase (or invent) the actual
# CLI implementation route. Runtime capabilities cross-check is still
# required before treating this table as evidence.
CLI_NATIVE_DEPENDENCY_ROUTES = {
    ("game", "rust"): "cli/src/commands/core/install/game.rs -> Lib/Rust adapter",
    ("iot", "rust"): "cli/src/commands/core/install/iot.rs -> Lib/Rust adapter",
    # Cloud CDK/Pulumi embed the MGC WebAdapter for JS package lifecycle;
    # this exception is framework-scoped. Terraform/Cloudflare remain
    # unsupported and do not inherit the cloud core's embedded route.
    # (Cloud CDK/Pulumi nhúng WebAdapter của MGC cho JS package lifecycle;
    # ngoại lệ theo framework, không áp dụng cho Terraform/Cloudflare.)
    ("clo", "cdk"): "adapters/cloud/src/adapter.rs -> embedded WebAdapter",
    ("clo", "pulumi"): "adapters/cloud/src/adapter.rs -> embedded WebAdapter",
}


def validate_adapter_consistency() -> int:
    """Cross-check the lane against its adapter or explicit CLI route.
    Generic adapter capability cannot disprove a separate MGC-native CLI
    route, but such an exception must name a real source file and is still
    checked operation-by-operation against the compiled binary.
    (Đối chiếu lane với adapter hoặc CLI route tường minh. Adapter chung
    không phủ định route MGC riêng, nhưng ngoại lệ phải trỏ tới source thật
    và vẫn bị cross-check từng operation với binary.)"""
    violations = []
    for lane in LANES:
        tag = f"{lane['core']}/{lane['language']}"
        caps = ADAPTER_REGISTRY_CAPABILITIES.get(lane["core"])
        if caps is None:
            # Unknown core row = the taxonomy cannot be verified against
            # the adapter — fail-closed instead of assuming.
            # (Core chưa có dòng bảng = không verify được taxonomy với
            # adapter — fail-closed thay vì giả định.)
            violations.append(
                f"{tag}: core '{lane['core']}' missing from "
                "ADAPTER_REGISTRY_CAPABILITIES — add the adapter's real row"
            )
            continue
        route_source = CLI_NATIVE_DEPENDENCY_ROUTES.get((lane["core"], lane["language"]))
        if route_source:
            # These CLI paths are verified against the binary's DepGate
            # operation table below; the generic adapter itself remains
            # unsupported for the registry protocol surface.
            source_path = route_source.split(" ->", 1)[0]
            source_path = os.path.join(
                os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                source_path,
            )
            if not os.path.isfile(source_path):
                violations.append(
                    f"{tag}: declared native CLI route source is missing: {route_source}"
                )
            continue
        owner = lane.get("install_owner", "")
        ops = lane.get("owner_by_operation", {})
        if not caps["install"] and owner == "native-engine":
            violations.append(
                f"{tag}: install_owner=native-engine but the production "
                "adapter returns Unsupported for install — false-positive "
                "taxonomy (P0-5)"
            )
        for op in ("resolve", "fetch"):
            op_owner = str(ops.get(op, ""))
            if not caps[op] and op_owner.startswith(("mgc", "magicore")):
                violations.append(
                    f"{tag}: owner_by_operation['{op}']='{op_owner}' but the "
                    f"production adapter returns Unsupported for {op} — "
                    "false-positive taxonomy (P0-5)"
                )
    if violations:
        for v in violations:
            print(f"ADAPTER CONSISTENCY GATE VIOLATION: {v}", file=sys.stderr)
        return 1
    print(
        f"adapter/CLI-route consistency gate: {len(LANES)} lanes have "
        "a declared implementation route (P0-5)"
    )
    return 0


def validate_dependency_owners() -> int:
    """P0-D dependency-owner gate (2026-09-16): every lane declares
    `dependency_owner` from the closed vocabulary, and an `mgc-native`
    claim REQUIRES the `native-engine` install_owner label — the two
    taxonomies must agree or the verdict gate below would be laundrable.
    Fail-closed: missing/unknown/inconsistent value blocks with exit 1.
    (Cổng P0-D (2026-09-16): mọi lane khai `dependency_owner` thuộc bộ giá
    trị đóng, và claim `mgc-native` BẮT BUỘC nhãn install_owner
    `native-engine` — hai taxonomy phải khớp nếu không cổng verdict bên
    dưới sẽ bị giặt. Fail-closed: thiếu/lạ/mâu thuẫn → exit 1.)"""
    violations = []
    for lane in LANES:
        tag = f"{lane['core']}/{lane['language']}"
        dep = lane.get("dependency_owner")
        if not dep:
            violations.append(f"{tag}: missing dependency_owner (P0-D)")
            continue
        if dep not in DEPENDENCY_OWNER_VOCABULARY:
            violations.append(
                f"{tag}: unknown dependency_owner '{dep}' (vocabulary: "
                f"{', '.join(DEPENDENCY_OWNER_VOCABULARY)})"
            )
        if dep == "mgc-native" and lane.get("install_owner") != "native-engine":
            violations.append(
                f"{tag}: dependency_owner=mgc-native but install_owner="
                f"'{lane.get('install_owner')}' — contradiction (P0-D)"
            )
    if violations:
        for v in violations:
            print(f"DEPENDENCY-OWNER GATE VIOLATION: {v}", file=sys.stderr)
        return 1
    print(
        f"dependency-owner gate: {len(LANES)} lanes clean "
        f"(vocabulary: {', '.join(DEPENDENCY_OWNER_VOCABULARY)})"
    )
    return 0


def check_dep_gate_consistency(binary_ownership: dict, lanes: list) -> list:
    """Cross-check every package operation against the compiled CLI truth.
    A coarse `install` match is insufficient: CRUD/list/frozen/offline/GC
    can differ by ecosystem. Missing binary cells and missing lane owners
    fail closed; an unsupported binary cell cannot be laundered as native.
    (Đối chiếu thuần: `dependency_owner` của lane matrix với bảng của
    binary. Cả hai chiều đều fail.)
    `binary_ownership`: {core: {"operations": {op: owner}, "languages":
    {lang: {op: owner}}, "frameworks": {id: {op: owner}}}. A lane with
    `framework_id` must match that exact qualification record; it cannot
    fall back to a broader language/core owner.
    Matrix lane languages map to gate languages (typescript->ts,
    react-native->rn, others identical).
    Returns the violation list (empty = consistent)."""
    def matches(binary_owner, matrix_owner):
        if binary_owner == "mgc-native":
            return matrix_owner == "mgc" or matrix_owner == "magicore-shared-cas"
        if binary_owner == "scaffold-only":
            return matrix_owner == "scaffold-only"
        return binary_owner == "unsupported" and str(matrix_owner).startswith("unsupported")
    # Matrix-taxonomy → gate-ecosystem ids, PER CORE: the matrix names a
    # lane by its toolchain language while the firewall matches the
    # lane's detected ecosystem id. (lib, typescript) scaffolds a
    # TypeScript project (gate "ts"); (app, react-native) scaffolds an RN
    # project (gate "rn"); (game, rust) scaffolds BEVY (gate "bevy");
    # (iot, rust) scaffolds esp32-rust (gate "esp32-rust"). Every other
    # (core, language) pair already spells the gate id.
    language_alias = {
        ("web", "javascript"): "js",
        ("web", "typescript"): "ts",
        ("lib", "typescript"): "ts",
        ("app", "react-native"): "rn",
        ("game", "rust"): "bevy",
        ("iot", "rust"): "esp32-rust",
    }
    violations = []
    for lane in lanes:
        tag = f"{lane['core']}/{lane['language']}"
        claimed = lane.get("dependency_owner")
        entry = binary_ownership.get(lane["core"])
        if entry is None:
            violations.append(f"{tag}: core missing from binary ownership table")
            continue
        gate_lang = language_alias.get((lane["core"], lane["language"]), lane["language"])
        truth_ops = None
        manifest_variant = lane.get("manifest_variant")
        if manifest_variant:
            truth_ops = entry.get("variants", {}).get(manifest_variant)
        framework_id = lane.get("framework_id")
        if framework_id:
            truth_ops = entry.get("frameworks", {}).get(framework_id)
            if not isinstance(truth_ops, dict):
                violations.append(
                    f"{tag}: framework '{framework_id}' missing from binary qualification table"
                )
                continue
        if truth_ops is None:
            truth_ops = entry.get("languages", {}).get(gate_lang)
        if truth_ops is None:
            truth_ops = entry.get("operations")
        if not isinstance(truth_ops, dict):
            violations.append(f"{tag}: operation ownership missing from binary table")
            continue
        matrix_ops = lane.get("owner_by_operation", {})
        binary_install = truth_ops.get("install")
        expected_claim = {
            "mgc-native": "mgc-native",
            "scaffold-only": "scaffold-only",
            "unsupported": "unsupported",
        }.get(binary_install)
        claim_matches = claimed == expected_claim or (
            binary_install == "unsupported" and claimed == "scaffold-only"
        )
        if not claim_matches:
            violations.append(
                f"{tag}: matrix claims '{claimed}' but binary install owner is "
                f"'{binary_install or 'missing'}'"
            )
        for operation in ALL_DEPENDENCY_OPERATIONS:
            binary_owner = truth_ops.get(operation)
            matrix_owner = matrix_ops.get(operation)
            if binary_owner is None:
                violations.append(f"{tag}: binary operation owner missing '{operation}'")
            elif matrix_owner is None:
                violations.append(f"{tag}: matrix operation owner missing '{operation}'")
            elif not matches(binary_owner, matrix_owner):
                violations.append(
                    f"{tag}: operation '{operation}' matrix owner '{matrix_owner}' "
                    f"does not match binary owner '{binary_owner}'"
                )
    return violations


def validate_dep_gate_consistency(mgc_bin=None) -> int:
    """T0.4 binary↔matrix consistency gate: shell `mgc capabilities` and
    require every lane's `dependency_owner` to agree with the C0 firewall
    table for the install op. It also captures the complete framework
    qualification inventory from this compiled binary; catalogued
    frameworks without native ownership and a lifecycle lane stay visible
    and block promotion. No binary fails closed in release-gating runs.
    (Cổng nhất quán T0.4: gọi `mgc capabilities` và bắt mọi lane khớp
    bảng tường lửa C0 ở op install. Không có binary → ghi UNAVAILABLE
    trung thực.)"""
    import subprocess

    # An explicitly selected binary is part of the evidence identity. Never
    # replace a missing path with a different local binary: that would let a
    # stale/debug binary silently satisfy a CI or release check.
    # (Binary được chỉ định là một phần identity của evidence; không fallback
    # sang binary debug khác khi đường dẫn được chọn bị thiếu.)
    env_bin = os.environ.get("MGC_BIN")
    candidates = (
        [mgc_bin]
        if mgc_bin is not None
        else [env_bin]
        if env_bin
        else ["./target/debug/mgc"]
    )
    binary = next((c for c in candidates if os.path.isfile(c)), None)
    if binary is None:
        if (
            os.environ.get("MGC_ALLOW_MISSING_DEP_GATE_BINARY") == "1"
            and not os.environ.get("CI")
        ):
            print(
                "dep-gate cross-check: STATIC-ONLY (explicit local opt-in; "
                "not release evidence)"
            )
            return 0
        print(
            "DEP-GATE CROSS-CHECK VIOLATION: no compiled mgc binary; "
            "runtime ownership validation is mandatory",
            file=sys.stderr,
        )
        return 1
    try:
        raw = run_text_capture(
            [binary, "capabilities"], capture_output=True, text=True, timeout=120
        )
    except (OSError, subprocess.SubprocessError) as err:
        print(f"DEP-GATE CROSS-CHECK VIOLATION: cannot run binary: {err}", file=sys.stderr)
        return 1
    if raw.returncode != 0:
        print(
            "DEP-GATE CROSS-CHECK VIOLATION: `mgc capabilities` failed: "
            f"{raw.stderr.strip()}",
            file=sys.stderr,
        )
        return 1
    global FRAMEWORK_QUALIFICATION_EVIDENCE
    try:
        payload = json.loads(raw.stdout)
        ownership = {}
        framework_rows = []
        for core in payload["cores"]:
            table = core["dependency_ownership"]
            def owners(cells):
                return {operation: value["owner"] for operation, value in cells.items()}

            entry = {
                "operations": owners(table["operations"]),
                "languages": {
                    language: owners(cells)
                    for language, cells in table.get("languages", {}).items()
                },
                "frameworks": {
                    row["framework"]: owners(row["dependency_ownership"])
                    for row in core.get("framework_qualification", [])
                },
            }
            for framework in core.get("framework_qualification", []):
                framework_rows.append({
                    "core": core["core"],
                    "framework": framework["framework"],
                    "status": framework["status"],
                    "dependency_ownership": framework["dependency_ownership"],
                    "evidence": framework.get("evidence", ""),
                    "required_manifest": framework.get("required_manifest"),
                })
            for variant, cells in table.get("manifest_variants", {}).items():
                language = variant.split("/", 1)[0]
                entry.setdefault("variants", {})[variant] = owners(cells)
            ownership[core["core"]] = entry
        FRAMEWORK_QUALIFICATION_EVIDENCE = sorted(
            framework_rows, key=lambda row: (row["core"], row["framework"])
        )
    except (ValueError, KeyError, TypeError) as err:
        print(
            "DEP-GATE CROSS-CHECK VIOLATION: unparseable capabilities "
            f"payload: {err}",
            file=sys.stderr,
        )
        return 1
    violations = check_dep_gate_consistency(ownership, LANES)
    if violations:
        for v in violations:
            print(f"DEP-GATE CROSS-CHECK VIOLATION: {v}", file=sys.stderr)
        return 1
    print(
        f"dep-gate cross-check: {len(LANES)} lanes agree with the binary; "
        f"captured {len(FRAMEWORK_QUALIFICATION_EVIDENCE)} framework records"
    )
    return 0


def framework_catalog_errors(catalog, lanes):
    """Require native-qualified frameworks to have full evidence and scope-outs to explain gaps.
    (Framework claim native phải đủ evidence; scope-out phải nêu rõ phần còn thiếu.)"""
    if not isinstance(catalog, list) or not catalog:
        return ["compiled capability output has no framework catalog"]
    errors = []
    by_key = {}
    valid_statuses = {"mgc-engine-path", "scaffold-only"}
    lane_keys = {
        (lane.get("core"), lane.get("framework_id"))
        for lane in lanes
        if isinstance(lane, dict) and lane.get("framework_id")
    }
    lane_by_key = {
        (lane.get("core"), lane.get("framework_id")): lane
        for lane in lanes
        if isinstance(lane, dict) and lane.get("framework_id")
    }
    for row in catalog:
        if not isinstance(row, dict):
            errors.append("framework catalog contains a malformed row")
            continue
        core, framework = row.get("core"), row.get("framework")
        if not isinstance(core, str) or not core or not isinstance(framework, str) or not framework:
            errors.append("framework catalog row has invalid identity")
            continue
        key = (core, framework)
        if key in by_key:
            errors.append(f"duplicate framework catalog identity {core}/{framework}")
            continue
        by_key[key] = row
        status = row.get("status")
        if not isinstance(status, str) or status not in valid_statuses:
            errors.append(f"{core}/{framework} has unknown qualification status {status!r}")
        elif status == "scaffold-only":
            evidence = row.get("evidence")
            if not isinstance(evidence, str) or not evidence.strip():
                errors.append(
                    f"{core}/{framework} scaffold-only entry is missing its scope-out reason"
                )
        if status == "mgc-engine-path" and key not in lane_keys:
            errors.append(f"{core}/{framework} has no lifecycle evidence lane")
        ownership = row.get("dependency_ownership")
        if not isinstance(ownership, dict):
            errors.append(f"{core}/{framework} has no dependency ownership table")
            continue
        unknown_ops = sorted(set(ownership) - set(ALL_DEPENDENCY_OPERATIONS))
        if unknown_ops:
            errors.append(
                f"{core}/{framework} has unknown dependency operations: "
                + ", ".join(unknown_ops)
            )
        for operation in ALL_DEPENDENCY_OPERATIONS:
            cell = ownership.get(operation)
            owner = cell.get("owner") if isinstance(cell, dict) else None
            valid_owners = {"mgc-native", "scaffold-only", "unsupported"}
            if owner not in valid_owners:
                errors.append(
                    f"{core}/{framework} operation {operation} has invalid owner "
                    f"{owner!r}"
                )
                continue
            lane = lane_by_key.get(key)
            if lane is not None:
                matrix_owners = lane.get("owner_by_operation")
                matrix_owner = (
                    matrix_owners.get(operation)
                    if isinstance(matrix_owners, dict)
                    else None
                )
                matches = (
                    owner == "mgc-native"
                    and matrix_owner in {"mgc", "magicore-shared-cas"}
                ) or (
                    owner == "scaffold-only" and matrix_owner == "scaffold-only"
                ) or (
                    owner == "unsupported"
                    and isinstance(matrix_owner, str)
                    and matrix_owner.startswith("unsupported")
                )
                if not matches:
                    errors.append(
                        f"{core}/{framework} operation {operation} differs from lifecycle owner "
                        f"(catalog={owner!r}, lane={matrix_owner!r})"
                    )
    for lane in lanes:
        if not isinstance(lane, dict) or not lane.get("framework_id"):
            continue
        key = (lane.get("core"), lane.get("framework_id"))
        if key not in by_key:
            errors.append(
                f"lifecycle lane {key[0]}/{key[1]} is not present in framework catalog"
            )
    return errors


def print_dependency_owner_summary() -> None:
    """Print declared ownership without implying runtime verification.
    (In ownership đã khai, không ám chỉ đã xác minh runtime.)
    Validate-only report includes evidence requirements and JSON on
    stdout so a consumer never parses the human table.
    (Báo cáo validate-only nêu ownership và evidence cần có, kèm JSON
    trên stdout để consumer không phải parse bảng người-đọc.)"""
    evidence_label = {
        "mgc-native": "native evidence required; not a verdict",
        "delegated": "compatibility only; never native",
        "scaffold-only": "unsupported",
        "unsupported": "unsupported",
    }
    print("=== dependency_owner summary (P0-D) ===")
    print(
        f"  {'lane':<24} {'dependency_owner':<16} "
        f"{'install_owner':<20} evidence status"
    )
    for lane in LANES:
        tag = f"{lane['core']}/{lane['language']}"
        print(
            f"  {tag:<24} {lane['dependency_owner']:<16} "
            f"{lane.get('install_owner', '-'):<20} "
            f"{evidence_label[lane['dependency_owner']]}"
        )
    print(
        json.dumps(
            {
                "dependency_owner_gate": "pass",
                "native_pm_verdict": "not-evaluated-without-lifecycle-evidence",
                "lanes": [
                    {
                        "core": lane["core"],
                        "language": lane["language"],
                        "dependency_owner": lane["dependency_owner"],
                        "native_pm_verdict": "not-evaluated-without-lifecycle-evidence",
                    }
                    for lane in LANES
                ],
            },
            indent=2,
        )
    )


# ---------------------------------------------------------------------------
# P0-3 platform-evidence counter I/O — see PLATFORM_EVIDENCE_PATH above.
# (I/O counter evidence platform P0-3 — xem PLATFORM_EVIDENCE_PATH bên trên.)
# ---------------------------------------------------------------------------


def _platform_evidence_load() -> dict:
    """Load the platform-evidence counter; {} when absent or unreadable
    (fresh checkout / first run). A corrupt file reads as {} — never a
    crash, never a fake streak.
    (Đọc counter evidence platform; {} khi chưa có hoặc hỏng (checkout
    mới / lần chạy đầu). File hỏng đọc thành {} — không bao giờ crash,
    không bao giờ giả chuỗi xanh.)"""
    try:
        with open(PLATFORM_EVIDENCE_PATH, "r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def _platform_evidence_write(evidence: dict) -> None:
    os.makedirs(os.path.dirname(PLATFORM_EVIDENCE_PATH) or ".", exist_ok=True)
    with open(PLATFORM_EVIDENCE_PATH, "w", encoding="utf-8") as f:
        json.dump(evidence, f, indent=2)
        f.write("\n")


def record_platform_green(os_name: str) -> int:
    """--record-green <os>: increment the CONSECUTIVE green-run counter
    for `os` and stamp the run SHA + UTC time. SHA source:
    MGC_PLATFORM_EVIDENCE_SHA env (CI passes GITHUB_SHA), falling back to
    `git rev-parse HEAD` for local runs.
    (--record-green <os>: tăng counter lần xanh LIÊN TIẾP cho `os` và đóng
    dấu SHA + giờ UTC của lần chạy. Nguồn SHA: env
    MGC_PLATFORM_EVIDENCE_SHA (CI truyền GITHUB_SHA), dự phòng
    `git rev-parse HEAD` cho lần chạy local.)"""
    if os_name not in PLATFORM_EVIDENCE_OS_NAMES:
        _fail(f"refusing to record unknown platform evidence key: {os_name}")
    matrix_path = os.environ.get(
        "MGC_LIFECYCLE_MATRIX_OUT",
        "docs/specs/lifecycleCapabilityMatrix.json",
    )
    try:
        with open(matrix_path, "r", encoding="utf-8") as matrix_file:
            matrix = json.load(matrix_file)
    except (OSError, ValueError) as exc:
        _fail(f"cannot read lifecycle matrix before recording platform green: {exc}")
    checkout = run_text_capture(
        ["git", "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        cwd=REPOSITORY_ROOT,
        check=False,
    )
    checkout_sha = checkout.stdout.strip() if checkout.returncode == 0 else ""
    current_tree_clean = lifecycle_working_tree_clean()
    expected_platform = PLATFORM_EVIDENCE_OS_NAMES[os_name]
    errors = platform_evidence_errors(
        matrix,
        checkout_sha,
        expected_platform=expected_platform,
        current_tree_clean=current_tree_clean,
    )
    workflow_sha = (os.environ.get("MGC_PLATFORM_EVIDENCE_SHA") or "").strip()
    commit_error = evidence_commit_error(
        checkout_sha,
        workflow_sha or None,
        is_ci=os.environ.get("CI", "").lower() in {"true", "1", "yes"},
        recorded_sha=workflow_sha or None,
        require_recorded=os.environ.get("CI", "").lower() in {"true", "1", "yes"},
    )
    if commit_error:
        errors.append(commit_error)
    if errors:
        _fail(
            f"refusing to record {os_name} as green: lifecycle evidence is not green",
            "\n".join(errors),
        )

    evidence = _platform_evidence_load()
    entry = evidence.get(os_name)
    if not isinstance(entry, dict):
        entry = {}
    entry["runs"] = int(entry.get("runs", 0)) + 1
    sha = workflow_sha or checkout_sha
    entry["last_green_sha"] = sha or "unknown"
    entry["last_green_at"] = datetime.datetime.now(
        datetime.timezone.utc
    ).strftime("%Y-%m-%dT%H:%M:%SZ")
    evidence[os_name] = entry
    _platform_evidence_write(evidence)
    print(
        f"platform evidence: {os_name} consecutive green runs = {entry['runs']} "
        f"(last_green_sha={entry['last_green_sha']}, last_green_at={entry['last_green_at']})"
    )
    if entry["runs"] >= PLATFORM_EVIDENCE_PROMOTION_THRESHOLD:
        print(
            f"platform evidence: {os_name} reached the promotion threshold "
            f"({PLATFORM_EVIDENCE_PROMOTION_THRESHOLD}) — flip it to a required gate now"
        )
    return 0


def reset_platform_counter(os_name: str) -> int:
    """--reset <os>: zero the consecutive-green counter — a failed run
    broke the streak and an honest counter must show it. The last_green_*
    fields stay: they are historical facts, not claims.
    (--reset <os>: về 0 counter lần xanh liên tiếp — lần chạy fail đã phá
    chuỗi và counter trung thực phải thể hiện điều đó. Trường last_green_*
    giữ nguyên: đó là sự thật lịch sử, không phải claim.)"""
    evidence = _platform_evidence_load()
    entry = evidence.get(os_name)
    if not isinstance(entry, dict):
        entry = {}
    entry["runs"] = 0
    evidence[os_name] = entry
    _platform_evidence_write(evidence)
    print(f"platform evidence: {os_name} consecutive-green counter reset to 0 (streak broken)")
    return 0


def platform_evidence_errors(
    matrix, checkout_sha, *, expected_platform=None, current_tree_clean=None
):
    """Return failures that must block a platform-green streak increment."""
    errors = []
    if not isinstance(matrix, dict) or matrix.get("matrix_kind") != "lifecycle":
        return ["matrix is missing or is not lifecycle evidence"]
    if matrix.get("schema_version") != SCHEMA_VERSION:
        errors.append(
            f"matrix schema_version {matrix.get('schema_version')!r} "
            f"does not match required schema {SCHEMA_VERSION}"
        )
    if matrix.get("working_tree_clean") is not True:
        errors.append(
            "matrix was generated from a dirty or unknown working tree; "
            "platform promotion requires a clean source checkout"
        )
    if current_tree_clean is not True:
        errors.append(
            "current checkout is dirty or Git could not verify cleanliness; "
            "platform promotion requires a clean source checkout"
        )
    matrix_sha = matrix.get("commit")
    if not checkout_sha or matrix_sha != checkout_sha:
        errors.append(
            f"matrix commit {matrix_sha!r} does not match checkout {checkout_sha!r}"
        )
    if expected_platform and matrix.get("platform") != expected_platform:
        errors.append(
            f"matrix platform {matrix.get('platform')!r} does not match "
            f"runner platform {expected_platform!r}"
        )
    lanes = matrix.get("lanes")
    if not isinstance(lanes, list):
        return errors + ["matrix lanes are missing or malformed"]
    indexed = {}
    for lane in lanes:
        if not isinstance(lane, dict):
            errors.append("matrix contains a malformed lane record")
            continue
        core_language = (lane.get("core"), lane.get("language"))
        framework_id = lane.get("framework_id", "") or ""
        if (
            not all(isinstance(part, str) and part for part in core_language)
            or not isinstance(framework_id, str)
        ):
            errors.append("matrix contains a lane with invalid core/language identity")
            continue
        key = (*core_language, framework_id)
        if key in indexed:
            errors.append(f"matrix contains duplicate lane {key}")
        indexed[key] = lane
    for core, language, framework_id in sorted(
        set(indexed) - PLATFORM_EVIDENCE_RELEASE_SCOPE
    ):
        tag = f"{core}/{language}" + (f"#{framework_id}" if framework_id else "")
        errors.append(f"matrix contains lane outside global v1.2 scope {tag}")
    for core, language, framework_id in sorted(PLATFORM_EVIDENCE_RELEASE_SCOPE):
        tag = f"{core}/{language}" + (f"#{framework_id}" if framework_id else "")
        lane = indexed.get((core, language, framework_id))
        if lane is None:
            errors.append(f"platform release-scope lane {tag} is missing")
            continue
        expected_scope_out_reason = release_scope_out_reason(core, language, framework_id)
        actual_scope_out_reason = lane.get("release_scope_out_reason")
        if actual_scope_out_reason != expected_scope_out_reason:
            errors.append(
                f"platform lane {tag} release scope-out reason differs from source contract"
            )
        scope_out = expected_scope_out_reason is not None
        if not scope_out and lane.get("toolchain_available") is not True:
            errors.append(f"platform lane {tag} toolchain is unavailable or unverified")
        expected_gate_role = "scoped-out" if scope_out else "release-blocking"
        if lane.get("gate_role") != expected_gate_role:
            errors.append(
                f"platform lane {tag} gate_role is {lane.get('gate_role')!r}, "
                f"expected {expected_gate_role!r}"
            )
        required = lane.get("required_dimensions")
        dimensions = lane.get("dimensions")
        if (
            not isinstance(required, list)
            or not required
            or not all(isinstance(name, str) and name for name in required)
            or len(set(required)) != len(required)
        ):
            errors.append(f"platform release-scope lane {tag} has no required dimensions")
            continue
        source_required = required_dimensions_for_lane(core, language, framework_id)
        if source_required is None:
            errors.append(f"platform release-scope lane {tag} has no source applicability contract")
            continue
        source_lane = next(
            (
                candidate for candidate in LANES
                if candidate["core"] == core
                and candidate["language"] == language
                and (candidate.get("framework_id", "") or "") == framework_id
            ),
            None,
        )
        if source_lane is None:
            errors.append(f"platform release-scope lane {tag} has no source owner contract")
            continue
        errors.extend(lifecycle_lane_owner_contract_errors(lane))
        if lane.get("evidence_only") is not bool(source_lane.get("evidence_only")):
            errors.append(
                f"platform lane {tag} evidence_only differs from source contract"
            )
        if tuple(required) != source_required:
            errors.append(
                f"platform release-scope lane {tag} required dimensions differ from source contract: "
                f"matrix={required!r}, source={list(source_required)!r}"
            )
        if not isinstance(dimensions, dict):
            errors.append(f"platform release-scope lane {tag} has malformed dimensions")
            continue
        unknown_dimensions = sorted(set(dimensions) - set(ALL_DIMENSIONS))
        if unknown_dimensions:
            errors.append(
                f"platform release-scope lane {tag} has unknown dimensions: "
                + ", ".join(unknown_dimensions)
            )
        missing_dimensions = sorted(set(ALL_DIMENSIONS) - set(dimensions))
        if missing_dimensions:
            errors.append(
                f"platform release-scope lane {tag} is missing v1.2 dimensions: "
                + ", ".join(missing_dimensions)
            )
        invalid_dimensions = sorted(
            name for name, status in dimensions.items() if status not in ALL_STATUSES
        )
        if invalid_dimensions:
            errors.append(
                f"platform release-scope lane {tag} has invalid dimension statuses: "
                + ", ".join(
                    f"{name}={dimensions.get(name)!r}" for name in invalid_dimensions
                )
            )
        enforced_required = source_required
        failed = [name for name in enforced_required if dimensions.get(name) not in PASS_STATUSES]
        if failed and not scope_out:
            errors.append(
                f"platform release-scope lane {tag} has non-passing required dimensions: "
                + ", ".join(f"{name}={dimensions.get(name, 'missing')}" for name in failed)
            )
        if not scope_out and lane.get("verdict") != "orchestration-lifecycle-passed":
            errors.append(
                f"platform release-scope lane {tag} verdict is "
                f"{lane.get('verdict')!r}, not orchestration-lifecycle-passed"
            )
        lifecycle_owners = lane.get("lifecycle_owners")
        if not isinstance(lifecycle_owners, dict):
            errors.append(f"platform lane {tag} is missing lifecycle owner evidence")
        else:
            unknown_owner_dimensions = sorted(set(lifecycle_owners) - set(ALL_DIMENSIONS))
            if unknown_owner_dimensions:
                errors.append(
                    f"platform lane {tag} has unknown lifecycle owner dimensions: "
                    + ", ".join(unknown_owner_dimensions)
                )
            invalid_owners = {
                dimension: owner for dimension, owner in lifecycle_owners.items()
                if not isinstance(owner, str) or owner not in LIFECYCLE_OWNER_VALUES
            }
            if invalid_owners:
                errors.append(
                    f"platform lane {tag} has invalid lifecycle owners: {invalid_owners!r}"
                )
            for dimension in source_required:
                owner = lifecycle_owners.get(dimension)
                if not isinstance(owner, str) or owner not in LIFECYCLE_OWNER_VALUES:
                    errors.append(
                        f"platform lane {tag} has missing or invalid lifecycle owner "
                        f"for {dimension}: {owner!r}"
                    )
                else:
                    source_owner = lifecycle_owner_for(source_lane, dimension)
                    if owner != source_owner:
                        errors.append(
                            f"platform lane {tag} lifecycle owner differs from source "
                            f"contract for {dimension}: matrix={owner!r}, "
                            f"source={source_owner!r}"
                        )
                if (
                    (not scope_out or lane.get("toolchain_available") is True)
                    and isinstance(owner, str)
                    and owner in LIFECYCLE_OWNER_VALUES
                    and not lifecycle_status_owner_matches(
                        dimensions.get(dimension), owner
                    )
                ):
                    errors.append(
                        f"platform lane {tag} status/owner mismatch for {dimension}: "
                        f"status={dimensions.get(dimension)!r}, owner={owner!r}"
                    )
        if lane.get("dependency_owner") != "mgc-native":
            errors.extend(native_pm_lane_errors(lane))
    errors.extend(native_pm_claim_errors(lanes))
    errors.extend(framework_catalog_errors(matrix.get("framework_catalog"), lanes))
    return errors


def lifecycle_environment(sandbox: str, project_path: str) -> dict[str, str]:
    """Keep MagiCore and common toolchain write paths inside each lane.
    (Giới hạn đường ghi MagiCore và cache toolchain phổ biến trong sandbox.)"""
    env = os.environ.copy()
    host_home = env.get("HOME") or os.path.expanduser("~")
    home = os.path.join(sandbox, ".home")
    cache = os.path.join(sandbox, ".cache")
    data = os.path.join(sandbox, ".data")
    temp = os.path.join(sandbox, ".tmp")
    cargo_home = os.path.join(sandbox, ".cargo-home")
    go_path = os.path.join(sandbox, ".go")
    env["HOME"] = home
    env["USERPROFILE"] = home
    # Windows dirs::home_dir can ignore HOME/USERPROFILE; pin the shared store.
    # Windows dirs::home_dir có thể bỏ qua HOME/USERPROFILE; ghim store riêng.
    env["MAGICORE_STORE_ROOT"] = os.path.join(home, ".magicore", "store")
    env["TMPDIR"] = temp
    env["TMP"] = temp
    env["TEMP"] = temp
    env["XDG_CACHE_HOME"] = cache
    env["XDG_CONFIG_HOME"] = os.path.join(home, ".config")
    env["XDG_DATA_HOME"] = data
    env["XDG_STATE_HOME"] = os.path.join(home, ".local", "state")
    env["PYTHONPYCACHEPREFIX"] = os.path.join(cache, "python-bytecode")
    env["CARGO_HOME"] = cargo_home
    env["CARGO_TARGET_DIR"] = os.path.join(sandbox, ".cargo-target")
    # Rustup's proxy binaries need the installed toolchain metadata even when
    # CARGO_HOME is lane-isolated. Keep that host toolchain read-only; never
    # fall back to downloading/installing a toolchain during matrix execution.
    # (Proxy rustup cần metadata toolchain đã cài dù CARGO_HOME được cô lập.
    # Chỉ đọc toolchain host; không tải/cài toolchain trong lúc chạy matrix.)
    rustup_home = env.get("RUSTUP_HOME")
    if not rustup_home or not os.path.isdir(rustup_home):
        rustup_home = os.path.join(host_home, ".rustup")
    if os.path.isdir(rustup_home):
        env["RUSTUP_HOME"] = rustup_home
    else:
        env.pop("RUSTUP_HOME", None)
    env["RUSTUP_AUTO_INSTALL"] = "0"
    env["PIP_CACHE_DIR"] = os.path.join(cache, "pip")
    env["UV_CACHE_DIR"] = os.path.join(cache, "uv")
    env["npm_config_cache"] = os.path.join(cache, "npm")
    env["DENO_DIR"] = os.path.join(cache, "deno")
    env["BUN_INSTALL_CACHE_DIR"] = os.path.join(cache, "bun")
    env["GOPATH"] = go_path
    env["GOMODCACHE"] = os.path.join(go_path, "pkg", "mod")
    env["GOCACHE"] = os.path.join(cache, "go-build")
    env["GRADLE_USER_HOME"] = os.path.join(cache, "gradle")
    env["DOTNET_CLI_HOME"] = os.path.join(home, ".dotnet")
    env["ANDROID_USER_HOME"] = os.path.join(home, ".android")
    env["PUB_CACHE"] = os.path.join(cache, "pub")
    env["NUGET_PACKAGES"] = os.path.join(cache, "nuget")
    env["SWIFT_MODULECACHE_PATH"] = os.path.join(cache, "swift-modules")
    env["MGC_CACHE_DIR"] = os.path.join(project_path, ".magicore")
    # Several tools (notably Go) require TMPDIR to exist before startup.
    # Create only lane-owned tool/cache directories; project state remains
    # owned by the real `mgc` commands under test.
    # (Một số tool, đặc biệt Go, cần TMPDIR tồn tại trước khi khởi chạy.
    # Chỉ tạo thư mục tool/cache thuộc lane; project state để mgc quản lý.)
    for key in (
        "HOME",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "PYTHONPYCACHEPREFIX",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
        "MAGICORE_STORE_ROOT",
        "PIP_CACHE_DIR",
        "UV_CACHE_DIR",
        "npm_config_cache",
        "DENO_DIR",
        "BUN_INSTALL_CACHE_DIR",
        "GOPATH",
        "GOMODCACHE",
        "GOCACHE",
        "GRADLE_USER_HOME",
        "DOTNET_CLI_HOME",
        "ANDROID_USER_HOME",
        "PUB_CACHE",
        "NUGET_PACKAGES",
        "SWIFT_MODULECACHE_PATH",
        "TMPDIR",
    ):
        os.makedirs(env[key], exist_ok=True)
    return env


def lifecycle_step_environment(
    lane: dict,
    step: str,
    lane_env: dict[str, str],
    flutter_sdk_pub_cache: str,
    flutter_sdk_home: str = "",
) -> dict[str, str]:
    """Use the warmed job-local Flutter SDK home/cache for test/build only.
    (Chỉ dùng HOME/cache Flutter SDK tạm của job cho test/build.)"""
    if (lane.get("core"), lane.get("language")) == ("app", "flutter") and step in {
        "test",
        "build",
    }:
        if not flutter_sdk_home:
            # Local/unwarmed runs keep the lane-private HOME and Pub cache.
            # Local/chưa warm thì giữ HOME và Pub cache riêng của lane.
            return lane_env
        # MgC project dependencies remain under its sandbox-owned store; this
        # home/cache matches the pre-warm step so Flutter does not bootstrap
        # its own SDK packages from a different HOME; it is not project resolution.
        # HOME/cache trùng bước warm để Flutter không bootstrap package SDK từ
        # HOME khác; chúng không dùng để resolve dependency của project.
        step_env = lane_env.copy()
        step_env["PUB_CACHE"] = flutter_sdk_pub_cache
        if flutter_sdk_home:
            step_env["HOME"] = flutter_sdk_home
            step_env["USERPROFILE"] = flutter_sdk_home
        return step_env
    return lane_env


def flutter_sdk_pub_cache_directory(
    host_environment: dict[str, str],
) -> str:
    """Resolve the job-local cache warmed for Flutter's SDK tool.
    (Tìm cache riêng của job đã dùng để làm nóng công cụ Flutter SDK.)"""
    configured = host_environment.get("PUB_CACHE")
    if configured:
        return os.path.abspath(configured)
    job_home = host_environment.get("FLUTTER_SDK_HOME")
    if job_home:
        return os.path.join(os.path.abspath(job_home), ".pub-cache")
    # Never substitute the developer/runner account cache for an isolated lane.
    # Không bao giờ thay cache riêng của lane bằng cache tài khoản developer/runner.
    return ""


def flutter_sdk_home_directory(host_environment: dict[str, str]) -> str:
    """Resolve the job-local HOME shared by Flutter warm and runtime steps.
    (Tìm HOME riêng của job dùng chung cho bước warm và Flutter runtime.)"""
    configured = host_environment.get("FLUTTER_SDK_HOME")
    if configured:
        return os.path.abspath(configured)
    # Local matrix runs without the CI warm step keep their existing lane HOME.
    # Chạy matrix local không có bước warm CI thì giữ HOME riêng hiện tại của lane.
    return ""


def pulumi_local_environment(sandbox: str, environment: dict[str, str]) -> dict[str, str]:
    """Isolate Pulumi state locally and remove inherited cloud credentials.
    (Cô lập state Pulumi trong sandbox và bỏ credential cloud từ host.)"""
    env = environment.copy()
    backend_root = os.path.join(sandbox, ".pulumi-backend")
    pulumi_home = os.path.join(sandbox, ".pulumi-home")
    os.makedirs(backend_root, exist_ok=True)
    os.makedirs(pulumi_home, exist_ok=True)
    env["PULUMI_BACKEND_URL"] = Path(backend_root).as_uri()
    env["PULUMI_HOME"] = pulumi_home
    env["PULUMI_CONFIG_PASSPHRASE"] = secrets.token_urlsafe(32)
    env["PULUMI_SKIP_UPDATE_CHECK"] = "true"
    for name in tuple(env):
        if name == "PULUMI_ACCESS_TOKEN" or name.startswith(
            ("AWS_", "AZURE_", "ARM_", "GOOGLE_", "CLOUDSDK_")
        ):
            env.pop(name, None)
    env["AWS_EC2_METADATA_DISABLED"] = "true"
    return env


def recovery_environment(sandbox: str, project_path: str) -> dict[str, str]:
    """Give crash-recovery probes a private store below their project.
    (Cấp store riêng bên dưới project cho probe phục hồi crash.)"""
    env = lifecycle_environment(sandbox, project_path)
    recovery_root = os.path.join(project_path, ".magicore-recovery")
    recovery_home = os.path.join(recovery_root, "home")
    env["HOME"] = recovery_home
    env["USERPROFILE"] = recovery_home
    env["MGC_CACHE_DIR"] = recovery_root
    return env


def python_venv_environment(environment: dict[str, str], venv_root: str) -> dict[str, str]:
    """Activate a lane-local Python venv without mutating the caller env.
    (Kích hoạt venv riêng của lane, không sửa env của tiến trình gọi.)"""
    env = environment.copy()
    scripts_dir = "Scripts" if os.name == "nt" else "bin"
    venv_bin = os.path.join(venv_root, scripts_dir)
    env["VIRTUAL_ENV"] = venv_root
    env["PIP_REQUIRE_VIRTUALENV"] = "true"
    env["PATH"] = venv_bin + os.pathsep + env.get("PATH", "")
    return env


def provision_python_build_tools(
    sandbox: str, project_path: str, environment: dict[str, str], timeout_s: int
) -> tuple[subprocess.CompletedProcess, dict[str, str]]:
    """Install pinned test/build tools only into a lane-owned virtualenv.
    (Cài tool test/build đã pin vào virtualenv riêng, không đụng Python host.)"""
    launcher = shutil.which("python", path=environment.get("PATH")) or shutil.which(
        "python3", path=environment.get("PATH")
    )
    if launcher is None:
        raise FileNotFoundError("python launcher not found on PATH")

    venv_root = os.path.join(sandbox, ".mgc-lifecycle-python")
    created = run_text_capture(
        [launcher, "-m", "venv", venv_root],
        capture_output=True,
        text=True,
        cwd=project_path,
        env=environment,
        timeout=timeout_s,
    )
    if created.returncode != 0:
        return created, environment

    scripts_dir = "Scripts" if os.name == "nt" else "bin"
    python_bin = os.path.join(
        venv_root, scripts_dir, "python.exe" if os.name == "nt" else "python"
    )
    venv_env = python_venv_environment(environment, venv_root)
    installed = run_text_capture(
        [
            python_bin,
            "-m",
            "pip",
            "install",
            "-q",
            "build==1.3.0",
            "pytest==8.4.2",
            "setuptools==80.9.0",
        ],
        capture_output=True,
        text=True,
        cwd=project_path,
        env=venv_env,
        timeout=timeout_s,
    )
    return installed, venv_env


def run_lane(mgc_bin: str, lane: dict) -> dict:
    """Execute one lane's lifecycle steps in a fresh sandbox; record
    per-dimension status: passed / delegated / failed / absent.
    Chạy các bước lifecycle của một lane trong sandbox mới; ghi trạng
    thái từng dimension: passed / delegated / failed / absent."""
    timeout_s = int(os.environ.get("MGC_LIFECYCLE_STEP_TIMEOUT", STEP_TIMEOUT_DEFAULT_S))
    sandbox = tempfile.mkdtemp(prefix=f"mgc-lc-{lane['core']}-{lane['language']}-")
    dims: dict[str, str] = {}
    detail = {"sandbox": sandbox}
    project_dir = lane.get("project_dir", lane["scaffold"][-1])
    project_path = os.path.join(sandbox, project_dir)
    lane_env = lifecycle_environment(sandbox, project_path)
    flutter_sdk_pub_cache = flutter_sdk_pub_cache_directory(os.environ)
    flutter_sdk_home = flutter_sdk_home_directory(os.environ)
    if (lane.get("core"), lane.get("language")) == ("clo", "pulumi"):
        lane_env = pulumi_local_environment(sandbox, lane_env)

    # Evidence-only lanes may depend on an external toolchain that is often
    # absent (terraform, cross rust targets, ...). Probe BEFORE the
    # lifecycle: a missing tool records EVERY dimension as `unverified` —
    # honest absence, never a pass, never a silent skip (P0-6).
    # (Lane evidence-only có thể phụ thuộc toolchain ngoài thường vắng
    # (terraform, cross rust target, ...). Probe TRƯỚC lifecycle: tool
    # thiếu thì MỌI dimension ghi `unverified` — vắng mặt trung thực,
    # không phải pass, không skip âm thầm (P0-6).)
    for probe_cmd in lane.get("toolchain_probes") or []:
        if shutil.which(probe_cmd) is None:
            dims = {dim: STATUS_UNVERIFIED for dim in ALL_DIMENSIONS}
            detail["toolchain_probe"] = (
                f"toolchain '{probe_cmd}' not found on PATH — "
                f"all dimensions recorded as unverified (P0-6 evidence-only lane)"
            )
            shutil.rmtree(sandbox, ignore_errors=True)
            return {"dims": dims, "detail": detail, "toolchain_available": False}

    def run_step(
        argv: list[str], subdir: str = "", lifecycle_step: str = ""
    ) -> tuple[int, str]:
        # Steps run INSIDE the scaffolded project (cwd = sandbox/<name>)
        # — install/test/build belong to the project, not the sandbox
        # root where two projects could otherwise collide.
        # Bước chạy BÊN TRONG project (cwd = sandbox/<name>) —
        # install/test/build thuộc project, không thuộc root sandbox.
        cwd = os.path.join(sandbox, subdir) if subdir else sandbox
        step_env = (
            lifecycle_step_environment(
                lane, lifecycle_step, lane_env, flutter_sdk_pub_cache, flutter_sdk_home
            )
            if lifecycle_step
            else lane_env
        )
        try:
            proc = run_text_capture(
                [mgc_bin] + argv, capture_output=True, text=True,
                cwd=cwd, env=step_env, timeout=timeout_s,
            )
            return proc.returncode, (proc.stdout or "") + (proc.stderr or "")
        except subprocess.TimeoutExpired:
            _fail(f"step timed out after {timeout_s}s: mgc {' '.join(argv)}")

    # create: the scaffold step proves the create dimension — BUT only
    # when the produced project matches the requested language. A
    # fallback template of another language (e.g. a Rust Cargo.toml
    # scaffolded for `create-lib java`) is a FAILED create, never a
    # passed one (P0-2 anti-fake-scaffold guard).
    # create: bước scaffold chứng minh dimension create — nhưng chỉ khi
    # project sinh ra khớp ngôn ngữ yêu cầu. Template fallback của ngôn
    # ngữ khác (vd Cargo.toml Rust cho `create-lib java`) là create
    # FAILED, không bao giờ passed (chống scaffold giả P0-2).
    rc, out = run_step(lane["scaffold"])

    # The scaffolded project directory (scaffold NAME arg) is where
    # every lifecycle step runs.
    # Thư mục project được scaffold (đối số NAME) là nơi chạy mọi bước.
    if rc != 0:
        dims["create"] = STATUS_FAILED
        # The scaffold never produced a project — detect was never
        # exercised: unverified, never a pass (fail-closed).
        # (Scaffold chưa sinh được project — detect chưa từng chạy:
        # unverified, không bao giờ pass (fail-closed).)
        dims["detect"] = STATUS_UNVERIFIED
        detail["create_output"] = out[-2000:]
    elif not scaffold_language_matches(
        sandbox, project_dir, lane["language"], lane.get("required_markers")
    ):
        dims["create"] = STATUS_FAILED
        # detect FAILED: the scaffold voice picked the WRONG toolchain —
        # this mismatch IS the detect dimension's failure evidence.
        # (detect FAILED: giọng scaffold chọn SAI toolchain — sự lệch này
        # chính là bằng chứng fail của dimension detect.)
        dims["detect"] = STATUS_FAILED
        found = sorted(
            f for f in os.listdir(os.path.join(sandbox, project_dir))
            if not f.startswith(".")
        )
        detail["create_output"] = (
            f"scaffold fell back to another language (requested "
            f"{lane['language']}); project files: {found}"
        )
    else:
        dims["create"] = STATUS_NATIVE
        # detect PASSED: the project carries a manifest of the REQUESTED
        # language — mgc's own scaffold/detect voice chose the right
        # toolchain (evidence: SCAFFOLD_MARKERS).
        # (detect PASSED: project có manifest của ĐÚNG ngôn ngữ yêu cầu —
        # giọng scaffold/detect của mgc chọn đúng toolchain (bằng chứng:
        # SCAFFOLD_MARKERS).)
        dims["detect"] = STATUS_NATIVE

    blocked_lifecycle_steps: dict[str, str] = {}
    for step in lane.get("pre_steps", []):
        # Lane preparation INSIDE the project BEFORE the lifecycle steps
        # (mgc subcommands or provisioned tools). A failing prep is a
        # FAILED lane prep — recorded, never skipped.
        # Chuẩn bị lane TRONG project TRƯỚC các bước lifecycle. Prep fail
        # là lane prep FAILED — ghi lại, không skip.
        if dims.get("create") != STATUS_NATIVE:
            break
        if step == "mgc_add_real_dependency":
            # Run the lane-declared fixture through mgc before install. If
            # it already exists in the manifest, leave resolution to native
            # install and do not claim an add operation ran. A failed add
            # blocks lifecycle steps so an empty project cannot pass.
            # (Chạy fixture do lane khai báo qua mgc trước install. Nếu
            # dependency đã có trong manifest, để native install xử lý và
            # không claim đã chạy add. Add lỗi chặn lifecycle để project
            # rỗng không thể được tính pass.)
            fixture = lane.get("dependency_fixture")
            if not isinstance(fixture, dict):
                _fail("pre_step 'mgc_add_real_dependency' requires dependency_fixture metadata")
            command = fixture.get("command")
            package = fixture.get("package")
            if (
                not isinstance(command, str)
                or not command
                or not isinstance(package, str)
                or not package
            ):
                _fail("dependency_fixture requires non-empty command and package strings")

            if "skip_if_declared" in fixture and not isinstance(
                fixture["skip_if_declared"], bool
            ):
                _fail("dependency_fixture skip_if_declared must be a boolean")
            if fixture.get("skip_if_declared"):
                manifest_path = fixture.get("manifest_path")
                manifest_name = fixture.get("manifest_name")
                if (
                    not isinstance(manifest_path, str)
                    or not manifest_path
                    or os.path.isabs(manifest_path)
                    or not isinstance(manifest_name, str)
                    or not manifest_name
                ):
                    _fail(
                        "dependency_fixture skip_if_declared requires a relative manifest_path "
                        "and manifest_name"
                    )
                project_root = os.path.abspath(project_path)
                manifest_file = os.path.abspath(
                    os.path.join(project_root, manifest_path)
                )
                try:
                    if os.path.commonpath((project_root, manifest_file)) != project_root:
                        _fail("dependency_fixture manifest_path must stay inside the project")
                except ValueError:
                    _fail("dependency_fixture manifest_path must stay inside the project")
                try:
                    with open(manifest_file, "r", encoding="utf-8") as manifest_stream:
                        manifest = json.load(manifest_stream)
                except (OSError, json.JSONDecodeError) as exc:
                    dims["add"] = STATUS_FAILED
                    detail["add_output"] = (
                        f"dependency fixture manifest {manifest_path!r} could not be read: {exc}"
                    )[-2000:]
                    for lifecycle_step, _ in lane["steps"]:
                        blocked_lifecycle_steps[lifecycle_step] = (
                            "required dependency fixture manifest was unreadable; "
                            "this step was not run without its prerequisite"
                        )
                    continue

                dependency_sections = (
                    manifest.get("dependencies", {}),
                    manifest.get("devDependencies", {}),
                    manifest.get("optionalDependencies", {}),
                    manifest.get("peerDependencies", {}),
                ) if isinstance(manifest, dict) else ()
                if any(
                    isinstance(section, dict) and manifest_name in section
                    for section in dependency_sections
                ):
                    detail["add_output"] = (
                        f"{manifest_name} is already declared in {manifest_path}; "
                        "MGC install will resolve and materialize it"
                    )
                    continue

            try:
                proc = run_text_capture(
                    [mgc_bin, command, package],
                    capture_output=True,
                    text=True,
                    cwd=project_path, env=lane_env, timeout=timeout_s,
                )
            except FileNotFoundError:
                _fail("pre_step 'mgc_add_real_dependency' requires the mgc binary — provisioning bug")
            except subprocess.TimeoutExpired:
                _fail(f"{command} dependency fixture setup timed out after {timeout_s}s")
            if proc.returncode != 0:
                dims["add"] = STATUS_FAILED
                detail["add_output"] = (
                    f"mgc {command} {package} (dependency fixture) failed: "
                    + (proc.stdout or "") + (proc.stderr or "")
                )[-2000:]
                for lifecycle_step, _ in lane["steps"]:
                    blocked_lifecycle_steps[lifecycle_step] = (
                        "required real-dependency fixture setup failed; "
                        "this step was not run against an empty project"
                    )
            else:
                dims["add"] = _owner_pass_status(lane)
                detail["add_output"] = (
                    f"MagiCore added dependency fixture {package}"
                )
        elif step == "provision_py_build_tools":
            # `python -m build` needs the build module; `mgc test`
            # needs pytest. Install with the LAUNCHER the lanes will
            # use (python if present, else python3 — the same
            # resolution mgc's build lane applies), so the module and
            # the launcher cannot drift apart across mixed-python
            # machines.
            # `python -m build` cần module build; `mgc test` cần
            # pytest. Cài bằng LAUNCHER mà lane sẽ dùng (python nếu
            # có, không thì python3 — đúng cách resolve của lane build
            # mgc), để module và launcher không lệch nhau trên máy
            # nhiều python.
            try:
                proc, lane_env = provision_python_build_tools(
                    sandbox, project_path, lane_env, timeout_s
                )
            except (FileNotFoundError, OSError):
                _fail("pre_step 'provision_py_build_tools' requires a python launcher on PATH")
            except subprocess.TimeoutExpired:
                _fail(f"py build tools install timed out after {timeout_s}s")
            if proc.returncode != 0:
                detail["toolchain_setup_output"] = (
                    "provision_py_build_tools failed: "
                    + (proc.stdout or "") + (proc.stderr or "")
                )[-2000:]
                for lifecycle_step in ("test", "build"):
                    if any(name == lifecycle_step for name, _ in lane["steps"]):
                        blocked_lifecycle_steps[lifecycle_step] = (
                            "lane-local test/build tool provisioning failed; "
                            "the step was not run without its prerequisite"
                        )
        elif step == "initialize_local_pulumi_stack":
            backend_url = lane_env.get("PULUMI_BACKEND_URL")
            if not backend_url or not backend_url.startswith("file://"):
                _fail("Pulumi lifecycle requires an isolated file backend URL")
            commands = (
                ["pulumi", "login", backend_url, "--non-interactive"],
                ["pulumi", "stack", "init", "lifecycle", "--non-interactive"],
            )
            setup_output = []
            setup_failed = False
            for command in commands:
                try:
                    proc = run_text_capture(
                        command,
                        capture_output=True,
                        text=True,
                        cwd=project_path,
                        env=lane_env,
                        timeout=timeout_s,
                    )
                except FileNotFoundError:
                    _fail("pre_step 'initialize_local_pulumi_stack' requires pulumi on PATH")
                except subprocess.TimeoutExpired:
                    _fail("Pulumi local backend setup timed out")
                setup_output.append((proc.stdout or "") + (proc.stderr or ""))
                if proc.returncode != 0:
                    setup_failed = True
                    break
            if setup_failed:
                detail["pulumi_stack_setup_output"] = "\n".join(setup_output)[-2000:]
                if any(name == "build" for name, _ in lane["steps"]):
                    blocked_lifecycle_steps["build"] = (
                        "local Pulumi stack setup failed; preview was not run"
                    )
        else:
            _fail(f"unknown pre_step: {step}")

    for step, argv in lane["steps"]:
        if step in blocked_lifecycle_steps:
            dims[step] = STATUS_UNVERIFIED
            detail[f"{step}_output"] = blocked_lifecycle_steps[step]
            continue
        if dims.get("create") != STATUS_NATIVE:
            # The lane broke before this step: SKIPPED, never a pass —
            # a skip records `unverified`, which satisfies nothing
            # (fail-closed, schema v7).
            # (Lane vỡ trước bước này: SKIP, không bao giờ là pass — skip
            # ghi `unverified`, không thỏa điều gì (fail-closed, schema v7).)
            dims[step] = STATUS_UNVERIFIED
            continue
        rc, out = run_step(argv, subdir=project_dir, lifecycle_step=step)
        if rc != 0:
            dims[step] = STATUS_FAILED
            detail[f"{step}_output"] = out[-2000:]
        else:
            # A successful wrapper command is not evidence of native
            # implementation. Record the declared owner of this operation;
            # external test/build/run toolchains remain delegated even when
            # MGC successfully orchestrates them.
            dims[step] = lifecycle_pass_status(lane, step)

    # `run` probe (P0 finding #6): long-lived production entry — start
    # `mgc run <script>`, wait for a REAL HTTP roundtrip on the probe
    # port, then shut down cleanly. A server that only starts (or dies
    # instantly) is a FAILED run, never a passed one.
    # Probe `run` (P0 finding #6): entry production dài hạn — khởi động
    # `mgc run <script>`, chờ HTTP roundtrip THẬT trên port probe, rồi
    # tắt sạch. Server chỉ khởi động (hoặc chết ngay) là run FAILED,
    # không bao giờ passed.
    probe = lane.get("run_probe")
    if probe is not None and dims.get("create") == STATUS_NATIVE:
        import urllib.request
        port = probe["port"]
        argv = ["run", probe["script"]]
        # New session/process group: killing the probe must kill the
        # WHOLE tree (mgc → node → vite) — a leaked vite child held the
        # port and broke later lanes/tests (zombie caught in REVIEW
        # round 2, 2026-09-12).
        # Session/process-group riêng: kill probe phải giết CẢ CÂY
        # (mgc → node → vite) — vite con sót lại giữ port và phá lane/
        # test sau đó (zombie bắt được ở REVIEW vòng 2).
        try:
            proc = subprocess.Popen(
                [mgc_bin] + argv,
                cwd=os.path.join(sandbox, project_dir),
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                env=lane_env,
                start_new_session=(os.name != "nt"),
            )
        except FileNotFoundError:
            _fail(f"cannot spawn mgc for run probe: {mgc_bin}")

        def _kill_tree(p):
            # POSIX: signal the whole process group (mgc + node + vite).
            # Windows: taskkill /T kills the tree; p.kill() is fallback.
            # POSIX: tín hiệu tới cả process group (mgc + node + vite).
            # Windows: taskkill /T giết cả cây; p.kill() là phương án dự phòng.
            if os.name != "nt":
                import signal
                try:
                    os.killpg(os.getpgid(p.pid), signal.SIGTERM)
                except (ProcessLookupError, PermissionError):
                    p.terminate()
            else:
                run_text_capture(
                    ["taskkill", "/T", "/F", "/PID", str(p.pid)],
                    capture_output=True,
                )
                p.terminate()

        def _sweep_port_leak():
            # The dev toolchain (node/vite) can end up RE-PARENTED into
            # its own session (observed: PPID=1, own PGID) — group kill
            # misses it. Sweep by PORT: find the PID still LISTENING on
            # the probe port and kill it directly. A leaked listener
            # breaks every later lane and test on this machine (REVIEW
            # round 2 finding, 2026-09-12).
            # Toolchain dev (node/vite) có thể bị RE-PARENT thành session
            # riêng (quan sát: PPID=1, PGID riêng) — kill group không trúng.
            # Quét theo PORT: tìm PID còn LISTEN trên port probe và giết
            # trực tiếp. Listener sót làm vỡ mọi lane/test sau đó.
            if os.name != "nt":
                try:
                    out = run_text_capture(
                        ["lsof", "-ti", f"tcp:{port}", "-sTCP:LISTEN"],
                        capture_output=True, text=True, timeout=10,
                    ).stdout.strip()
                    for pid_s in out.split():
                        if pid_s.isdigit():
                            try:
                                os.kill(int(pid_s), 9)
                            except (ProcessLookupError, PermissionError):
                                pass
                except (FileNotFoundError, subprocess.TimeoutExpired):
                    pass

        served = False
        deadline = time.monotonic() + min(timeout_s, 90)
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                # Server exited before ever serving — a real failure.
                # Server thoát trước khi phục vụ — fail thật.
                out = (proc.stdout.read() if proc.stdout else "") or ""
                dims["run"] = STATUS_FAILED
                detail["run_output"] = out[-2000:]
                break
            # Probe BOTH stacks: vite listens on ::1 (IPv6 localhost)
            # on some hosts and 127.0.0.1 on others — the roundtrip must
            # not depend on which stack the dev server picked.
            # Probe CẢ HAI stack: vite listen trên ::1 (IPv6 localhost)
            # ở một số máy và 127.0.0.1 ở máy khác — roundtrip không
            # được phụ thuộc stack mà dev server chọn.
            for host in ("127.0.0.1", "localhost"):
                try:
                    with urllib.request.urlopen(
                        f"http://{host}:{port}/", timeout=2
                    ) as resp:
                        if resp.status == 200:
                            served = True
                            break
                except Exception:
                    pass
            if served:
                break
            time.sleep(1)
        if proc.poll() is None:
            _kill_tree(proc)
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                _kill_tree(proc)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
        # Port sweep: the re-parented listener must be gone or the lane
        # leaks the port (breaks later lanes AND unrelated tests that
        # bind 4315, e.g. registry-server tests).
        # Quét port: listener bị re-parent phải biến mất, nếu không lane
        # rò port (phá lane sau và cả test không liên quan bind 4315,
        # vd test registry-server).
        _sweep_port_leak()
        if dims.get("run") != STATUS_FAILED:
            dims["run"] = lifecycle_pass_status(lane, "run") if served else STATUS_FAILED
            if not served:
                detail["run_output"] = (
                    "run probe: no HTTP 200 on localhost:%d before timeout" % port
                )

    # `dev` probe (P0-mới-1, Tech Lead vòng-6/7): spawn `mgc dev`, wait
    # for bind + HTTP 200, EDIT a source file, expect NEW output
    # (rebuild), check the HMR websocket endpoint responds, then kill the
    # tree + sweep the port. Reuses the same kill-tree/sweep discipline
    # as the run probe. A dev server that only prints help (the old
    # step) can never pass this.
    # Probe `dev` (P0-mới-1): spawn `mgc dev`, chờ bind + HTTP 200, SỬA
    # file source, chờ output MỚI (rebuild), kiểm tra endpoint websocket
    # HMR phản hồi, rồi kill cây + quét port. Tái dùng đúng kỷ luật
    # kill-tree/sweep như probe run. Dev server chỉ in help (bước cũ)
    # không bao giờ pass được.
    devp = lane.get("dev_probe")
    if devp is not None and dims.get("create") == STATUS_NATIVE:
        import urllib.request
        port = devp["port"]
        project_path = os.path.join(sandbox, project_dir)
        try:
            dev_proc = subprocess.Popen(
                [mgc_bin, "dev"],
                cwd=project_path,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                env=lane_env,
                start_new_session=(os.name != "nt"),
            )
        except FileNotFoundError:
            _fail(f"cannot spawn mgc for dev probe: {mgc_bin}")

        dev_served = False
        hmr_ok = False
        rebuilt = False
        deadline = time.monotonic() + min(timeout_s, 120)

        def _dev_kill_tree():
            # Same discipline as the run probe: group kill, then port
            # sweep for re-parented listeners.
            # Cùng kỷ luật probe run: kill group, rồi quét port cho
            # listener bị re-parent.
            if os.name != "nt":
                import signal
                try:
                    os.killpg(os.getpgid(dev_proc.pid), signal.SIGTERM)
                except (ProcessLookupError, PermissionError):
                    dev_proc.terminate()
            else:
                run_text_capture(
                    ["taskkill", "/T", "/F", "/PID", str(dev_proc.pid)],
                    capture_output=True,
                )
                dev_proc.terminate()

        def _dev_sweep_port():
            if os.name != "nt":
                try:
                    out = run_text_capture(
                        ["lsof", "-ti", f"tcp:{port}", "-sTCP:LISTEN"],
                        capture_output=True, text=True, timeout=10,
                    ).stdout.strip()
                    for pid_s in out.split():
                        if pid_s.isdigit():
                            try:
                                os.kill(int(pid_s), 9)
                            except (ProcessLookupError, PermissionError):
                                pass
                except (FileNotFoundError, subprocess.TimeoutExpired):
                    pass

        output_before_edit = ""
        while time.monotonic() < deadline:
            if dev_proc.poll() is not None:
                out = (dev_proc.stdout.read() if dev_proc.stdout else "") or ""
                dims["dev"] = STATUS_FAILED
                detail["dev_output"] = ("dev probe: server exited early — " + out)[-2000:]
                break
            for host in ("127.0.0.1", "localhost"):
                try:
                    with urllib.request.urlopen(
                        f"http://{host}:{port}/", timeout=2
                    ) as resp:
                        if resp.status == 200:
                            dev_served = True
                            break
                except Exception:
                    pass
            if dev_served:
                # HMR endpoint must respond (101 upgrade or 400/405 —
                # anything but connection-refused proves the WS route is
                # wired; a plain GET to a WS route cannot complete the
                # handshake but MUST be accepted by the server).
                # Endpoint HMR phải phản hồi (101 upgrade hoặc 400/405 —
                # bất cứ gì khác connection-refused chứng minh route WS
                # được wire; GET thường tới route WS không hoàn tất
                # handshake nhưng server PHẢI chấp nhận).
                for hmr_host in ("127.0.0.1", "localhost"):
                    try:
                        urllib.request.urlopen(
                            f"http://{hmr_host}:{port}{devp['hmr_path']}", timeout=2
                        )
                        hmr_ok = True
                    except urllib.error.HTTPError:
                        # 4xx from the route itself = the endpoint EXISTS.
                        # (4xx từ chính route = endpoint TỒN TẠI.)
                        hmr_ok = True
                    except Exception:
                        pass
                    if hmr_ok:
                        break
                break
            time.sleep(1)

        if dev_served and dev_proc.poll() is None:
            # Drain output so far, then edit the source — the dimension
            # demands a REBUILD, not just a boot.
            # Xả output đến giờ, rồi sửa source — dimension đòi REBUILD,
            # không chỉ khởi động.)
            import threading
            output_chunks: list[str] = []

            def _reader():
                try:
                    for line in dev_proc.stdout:
                        output_chunks.append(line)
                except Exception:
                    pass

            reader = threading.Thread(target=_reader, daemon=True)
            reader.start()
            output_before_edit = "".join(output_chunks)
            src = os.path.join(project_path, devp["source_file"])
            if os.path.isfile(src):
                with open(src, "a", encoding="utf-8") as f:
                    f.write(devp["source_edit"])
            rebuild_deadline = time.monotonic() + 30
            while time.monotonic() < rebuild_deadline:
                # New output after the edit = the watcher saw the change
                # and recompiled/reloaded.
                # (Output mới sau khi sửa = watcher thấy thay đổi và
                # biên dịch lại/reload.)
                if "".join(output_chunks) != output_before_edit:
                    rebuilt = True
                    break
                if dev_proc.poll() is not None:
                    break
                time.sleep(1)
            reader.join(timeout=2)

        if dev_proc.poll() is None:
            _dev_kill_tree()
            try:
                dev_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                _dev_kill_tree()
                try:
                    dev_proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    dev_proc.kill()
        _dev_sweep_port()

        if dims.get("dev") != STATUS_FAILED:
            if dev_served and rebuilt and hmr_ok:
                dims["dev"] = lifecycle_pass_status(lane, "dev")
            else:
                dims["dev"] = STATUS_FAILED
                detail["dev_output"] = (
                    f"dev probe: served={dev_served} hmr_endpoint={hmr_ok} "
                    f"rebuilt_after_edit={rebuilt} — the dimension requires ALL three"
                )

    # ------------------------------------------------------------------
    # Gate 11-C fine-grained install evidence: resolve / lock / fetch /
    # materialize — derived from the install OUTCOME plus observable
    # filesystem artifacts, never from intent. No artifact observed →
    # `unverified` (a zero-dep template is an honesty boundary, not a
    # pass); install failure → `failed` for the whole chain.
    # (Bằng chứng install tinh Gate 11-C: resolve/lock/fetch/materialize —
    # suy từ KẾT QUẢ install cộng artifact filesystem quan sát được,
    # không bao giờ suy từ ý định. Không quan sát thấy artifact →
    # `unverified` (template zero-dep là một biên trung thực, không phải
    # pass); install fail → `failed` cho cả chuỗi.)
    install_status = dims.get("install")
    project_path = os.path.join(sandbox, project_dir)
    if install_status == STATUS_FAILED:
        dims["resolve"] = STATUS_FAILED
        dims["lock"] = STATUS_FAILED
        dims["fetch"] = STATUS_FAILED
        dims["materialize"] = STATUS_FAILED
    elif install_status in PASS_STATUSES:
        owner = _owner_pass_status(lane)
        # lock: a known lockfile artifact EXISTS after install.
        # (lock: một artifact lockfile đã biết TỒN TẠI sau install.)
        probes = _lockfile_candidates(project_path, lane["language"])
        found, with_entries = [], []
        for name, kind in probes:
            probe_path = os.path.join(project_path, name)
            if not os.path.isfile(probe_path):
                continue
            found.append(name)
            count = _lockfile_entry_count(probe_path, kind)
            if count:
                with_entries.append(f"{name}({count})")
        if found:
            dims["lock"] = owner
            detail["lock_output"] = f"lockfile artifacts present: {found}"
        else:
            dims["lock"] = STATUS_UNVERIFIED
            detail["lock_output"] = (
                f"no lockfile artifact observed for language "
                f"'{lane['language']}' (zero-dep template or lockless "
                f"ecosystem) — claim unverified"
            )
        # resolve: the lockfile carries ≥1 resolved entry.
        # (resolve: lockfile mang ít nhất 1 entry đã resolve.)
        if with_entries:
            dims["resolve"] = owner
            detail["resolve_output"] = f"resolved entries observed: {with_entries}"
        else:
            dims["resolve"] = STATUS_UNVERIFIED
            detail["resolve_output"] = (
                "no resolved package entries observed — resolve claim unverified"
            )
        # materialize: the dependency tree landed where the ecosystem
        # actually puts it.
        # (materialize: cây dependency đã về đúng nơi ecosystem đặt nó.)
        label, marker = _materialize_marker(
            project_path, lane["language"], environment=lane_env
        )
        if label is None:
            dims["materialize"] = STATUS_UNVERIFIED
            detail["materialize_output"] = (
                "no observable materialization marker for this ecosystem"
            )
        elif _dir_nonempty(marker):
            dims["materialize"] = owner
            detail["materialize_output"] = f"materialized: {label} non-empty"
        else:
            dims["materialize"] = STATUS_UNVERIFIED
            detail["materialize_output"] = f"marker '{label}' absent or empty"
        # fetch: proven ONLY when bytes demonstrably landed — resolved
        # entries AND materialization together; one without the other is
        # unverified (never auto-passed).
        # (fetch: chỉ chứng minh khi byte thật sự về — entry đã resolve
        # VÀ materialize CÙNG lúc; thiếu một là unverified (không bao giờ
        # tự động pass).)
        if dims["resolve"] == owner and dims["materialize"] == owner:
            dims["fetch"] = owner
        else:
            dims["fetch"] = STATUS_UNVERIFIED
            detail["fetch_output"] = (
                f"fetch needs resolve+materialize evidence together "
                f"(resolve={dims['resolve']}, materialize={dims['materialize']})"
            )
    else:
        # install never ran (lane skipped) — the chain stays unverified.
        # (install chưa chạy (lane skip) — chuỗi giữ unverified.)
        for dim in ("resolve", "lock", "fetch", "materialize"):
            dims.setdefault(dim, STATUS_UNVERIFIED)

    # optimizer: run the REAL `mgc optimizer` in the sandbox. An exit-0
    # alone proves NOTHING (the command exits 0 with "Optimizer skipped"
    # when no runtime is detected) — the pass requires an actually-applied
    # configuration count > 0; no runtime / no optimizations → honestly
    # `unsupported` for this lane; command failure → `failed`.
    # (optimizer: chạy `mgc optimizer` THẬT trong sandbox. exit-0 trần
    # không chứng minh gì (lệnh exit-0 với "Optimizer skipped" khi không
    # thấy runtime) — pass đòi số cấu hình áp dụng THẬT > 0; không thấy
    # runtime / không có tối ưu → `unsupported` trung thực cho lane; lệnh
    # fail → `failed`.)
    if dims.get("create") == STATUS_NATIVE:
        rc, out = run_step(["optimizer"], subdir=project_dir)
        detail["optimizer_output"] = out[-2000:]
        applied = re.search(r"applied (\d+)/", out)
        if rc == 0 and applied and int(applied.group(1)) > 0:
            dims["optimizer"] = STATUS_NATIVE
        elif rc == 0:
            dims["optimizer"] = STATUS_UNSUPPORTED
        else:
            dims["optimizer"] = STATUS_FAILED

    # recovery — THE flagship dimension (Gate 11-C): real crash + real
    # repair through the built-in failpoint handshake. `mgc install` parks
    # at after-generation-begin (READY marker fsync'd by the binary), the
    # harness SIGKILLs it, then `mgc store doctor --repair` must leave the
    # store HEALTHY with 0 stale staging (Rust contract:
    # cli/tests/kill_injection_matrix.rs). Only lanes explicitly marked
    # `recovery_probe: true` (web/js and lib/ts using web.install) HAVE this
    # crash surface; native ownership alone is not recovery evidence.
    # Other lanes are honestly unsupported. POSIX only: no signal support
    # → unverified, never faked.
    # (recovery — dimension QUAN TRỌNG NHẤT (Gate 11-C): crash thật + sửa
    # chữa thật qua handshake failpoint có sẵn. `mgc install` đỗ tại
    # after-generation-begin (marker READY do binary fsync), harness
    # SIGKILL, rồi `mgc store doctor --repair` phải để store HEALTHY với
    # 0 stale staging (hợp đồng Rust: cli/tests/kill_injection_matrix.rs).
    # Chỉ lane khai `recovery_probe: true` (web/js và lib/ts dùng đúng
    # web.install) CÓ bề mặt crash này; native ownership không tự chứng
    # minh recovery. Lane khác unsupported trung thực. Chỉ POSIX: không
    # hỗ trợ signal → unverified, không bao giờ bịa.)
    if not lane.get("recovery_probe", False):
        dims["recovery"] = STATUS_UNSUPPORTED
        detail["recovery_output"] = (
            "no lane-specific crash-recovery probe is implemented; "
            "native dependency ownership does not imply recovery evidence"
        )
    elif os.name == "nt":
        dims["recovery"] = STATUS_UNVERIFIED
        detail["recovery_output"] = (
            "no POSIX signal support on this platform — probe not run, not faked"
        )
    elif dims.get("create") != STATUS_NATIVE or install_status not in PASS_STATUSES:
        dims["recovery"] = STATUS_UNVERIFIED
        detail["recovery_output"] = (
            "lane prerequisites (create+install pass) not met — probe not run"
        )
    elif _recovery_probe(mgc_bin, project_path, sandbox, timeout_s, detail):
        dims["recovery"] = STATUS_NATIVE
    else:
        dims["recovery"] = STATUS_FAILED

    # audit rides the audit matrix — reference only, never inferred.
    # audit thuộc matrix audit riêng — tham chiếu, không suy ra.
    dims["audit"] = STATUS_UNVERIFIED
    detail["audit_output"] = (
        "audit capability is proven by the separate audit capability "
        "matrix; this lifecycle matrix does not verify it"
    )

    # Dimensions with no step/probe for this lane: `unsupported` (honest —
    # `run`/`dev` on non-web lanes; the `store`/`offline` probes are not
    # part of any lane's step sequence today).
    # (Dimension không có bước/probe cho lane này: `unsupported` (trung
    # thực — `run`/`dev` trên lane non-web; probe `store`/`offline` chưa
    # thuộc step sequence của lane nào hiện nay).)
    for dim in ALL_DIMENSIONS:
        dims.setdefault(dim, STATUS_UNSUPPORTED)

    # Integrity verification is an explicit evidence dimension, not inferred
    # from fetching or a source-level implementation. No lane-specific
    # tampered-artifact probe exists in this matrix yet, so record unverified.
    # (Xác minh integrity là dimension evidence riêng, không suy từ fetch hay
    # code tồn tại. Matrix chưa có probe artifact bị sửa theo từng lane nên ghi
    # unverified.)
    dims["verify"] = STATUS_UNVERIFIED
    detail["verify_output"] = (
        "no lane-specific tampered-artifact rejection probe is implemented; "
        "source-level verifier presence is not runtime evidence"
    )

    # CRUD/list/frozen/offline/GC have no lane-specific execution probes yet.
    # Keep each operation explicit and unverified; install evidence cannot
    # stand in for the rest of the package-manager contract.
    # (Chưa có probe riêng CRUD/list/frozen/offline/GC theo lane. Ghi từng
    # operation là unverified; evidence install không thay thế phần còn lại.)
    for operation in NATIVE_PM_USER_OPERATIONS:
        if operation == "install":
            continue
        if operation in dims and dims[operation] != STATUS_UNSUPPORTED:
            continue
        dims[operation] = STATUS_UNVERIFIED
        detail[f"{operation.replace('-', '_')}_output"] = (
            "no lane-specific execution probe exists for this package operation"
        )

    shutil.rmtree(sandbox, ignore_errors=True)
    return {"dims": dims, "detail": detail, "toolchain_available": True}


def main() -> int:
    # P0-3 platform-evidence CLI: --record-green <os> / --reset <os> write
    # the gitignored streak counter consumed by the matrix JSON
    # (platform_evidence) and the CI promotion job. Kept in THIS script so
    # the counter schema has exactly one home.
    # (CLI evidence platform P0-3: --record-green <os> / --reset <os> ghi
    # counter gitignored mà JSON matrix (platform_evidence) và job
    # promotion CI tiêu thụ. Đặt trong CHÍNH script này để schema counter
    # chỉ có một ngôi nhà duy nhất.)
    cli_args = sys.argv[1:]
    is_ci = os.environ.get("GITHUB_ACTIONS", "").lower() == "true"
    commit = run_text_capture(
        ["git", "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        cwd=REPOSITORY_ROOT,
    ).stdout.strip()
    source_tree_clean_before_run = lifecycle_working_tree_clean()
    if cli_args:
        if cli_args[0] in ("--record-green", "--reset") and len(cli_args) == 2:
            if cli_args[0] == "--record-green":
                provenance_error = evidence_commit_error(
                    commit,
                    os.environ.get("GITHUB_SHA"),
                    is_ci=is_ci,
                    recorded_sha=os.environ.get("MGC_PLATFORM_EVIDENCE_SHA"),
                    require_recorded=True,
                )
                if provenance_error:
                    _fail(provenance_error)
                return record_platform_green(cli_args[1])
            # Reset only removes a positive streak claim. Keep this recovery
            # path available even after a SHA mismatch so failed/misbound CI
            # cannot leave an old green streak looking consecutive.
            return reset_platform_counter(cli_args[1])
        _fail(
            f"unknown arguments: {' '.join(cli_args)} "
            "(expected --record-green <os> / --reset <os>)"
        )
    provenance_error = evidence_commit_error(
        commit,
        os.environ.get("GITHUB_SHA"),
        is_ci=is_ci,
    )
    if provenance_error:
        _fail(provenance_error)
    # Resolve to an ABSOLUTE path: steps run with cwd = sandbox, so a
    # relative binary path would resolve inside the sandbox and vanish.
    # Quy về đường dẫn TUYỆT ĐỐI: bước chạy với cwd = sandbox nên đường
    # dẫn tương đối sẽ trỏ vào sandbox và biến mất.
    mgc_bin = os.environ.get("MGC_LIFECYCLE_BIN", "./target/debug/mgc")
    # Validation dry-run mode (Gate 11-C): check lane OWNER metadata
    # WITHOUT running any lifecycle step — the CI gate that blocks a
    # lane whose taxonomy is missing or self-contradictory (install
    # owner native-engine but fetch owned by cargo, etc.).
    # (Chế độ dry-run validate (Gate 11-C): kiểm tra metadata OWNER của
    # lane KHÔNG chạy bước lifecycle nào — cổng CI chặn lane thiếu
    # taxonomy hoặc tự mâu thuẫn (install owner native-engine mà fetch
    # thuộc cargo, v.v.).)
    if os.environ.get("MGC_LIFECYCLE_VALIDATE_ONLY") == "1":
        # P0-5: BOTH gates run here — owner taxonomy AND adapter
        # consistency — fail-closed on the first violation.
        # (P0-5: CHẠY CẢ HAI cổng ở đây — taxonomy owner VÀ nhất quán
        # adapter — fail-closed ngay vi phạm đầu tiên.)
        owner_rc = validate_lane_owners()
        adapter_rc = validate_adapter_consistency()
        # P0-D: the dependency-owner gate joins the validate-only set;
        # the per-lane summary table (with verdict ceilings + JSON) only
        # prints when every gate is clean — a broken taxonomy never gets
        # a table that looks like a report.
        # (P0-D: cổng dependency-owner gia nhập bộ validate-only; bảng
        # summary theo lane (kèm trần verdict + JSON) chỉ in khi mọi cổng
        # sạch — taxonomy vỡ không bao giờ được có cái bảng trông như
        # báo cáo.)
        dep_rc = validate_dependency_owners()
        # T0.4: the binary↔matrix consistency gate joins the validate-only
        # set (mgc_bin resolved above from MGC_LIFECYCLE_BIN or default).
        # (T0.4: cổng nhất quán binary↔matrix gia nhập bộ validate-only.)
        dep_gate_rc = validate_dep_gate_consistency(mgc_bin)
        if owner_rc or adapter_rc or dep_rc or dep_gate_rc:
            return 1
        print_dependency_owner_summary()
        return 0
    mgc_bin = os.path.abspath(mgc_bin)
    if not os.path.isfile(mgc_bin) and os.name == "nt" and not mgc_bin.endswith(".exe"):
        # Windows binaries carry the .exe suffix — retry the suffixed path
        # before failing (the workflow passes the POSIX spelling).
        # (Binary Windows có hậu tố .exe — thử đường dẫn có hậu tố trước
        # khi fail.)
        candidate = mgc_bin + ".exe"
        if os.path.isfile(candidate):
            mgc_bin = candidate
    if not os.path.isfile(mgc_bin):
        _fail(f"mgc binary not found at {mgc_bin} — build it first")

    # Owner metadata gate (Gate 11-C, vòng-11 verdict): every lane MUST
    # declare per-operation owners BEFORE any collection runs — a lane
    # whose taxonomy is incomplete would emit a matrix that looks
    # machine-readable while laundering "install passed" into
    # "eccosystem managed" (the exact audit finding).
    # (Cổng metadata owner: mọi lane PHẢI khai báo owner theo từng
    # operation TRƯỚC khi collect — lane thiếu taxonomy sẽ sinh matrix
    # trông máy-đọc-được nhưng giặt "install passed" thành "ecosystem
    # được quản lý" (đúng finding của audit).)
    # Fail-closed (P0-5): a violated gate REFUSES collection — the old call
    # ignored the return code, letting mislabeled taxonomy reach the matrix
    # despite the comment above promising otherwise.
    # (Fail-closed (P0-5): cổng vi phạm TỪ CHỐI collect — code cũ bỏ qua
    # return code, cho taxonomy gắn nhãn sai chạm tới matrix dù comment
    # bên trên hứa điều ngược lại.)
    dep_gate_rc = validate_dep_gate_consistency(mgc_bin)
    if (
        validate_lane_owners() != 0
        or validate_adapter_consistency() != 0
        or validate_dependency_owners() != 0
        or dep_gate_rc != 0
    ):
        _fail(
            "lane owner/adapter-consistency/dependency-owner gates failed — "
            "refusing to collect a matrix from mislabeled taxonomy"
        )

    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    results = []
    for lane in LANES:
        r = run_lane(mgc_bin, lane)
        # Failed-dimension output is PRINTED (CI log must show WHY a
        # lane failed) and stored in the artifact (the JSON keeps the
        # evidence next to the verdict).
        # Output của dimension fail được IN ra (log CI phải cho thấy vì
        # sao lane fail) và lưu vào artifact (JSON giữ bằng chứng cạnh
        # verdict).
        for key, out in r["detail"].items():
            if key.endswith("_output"):
                print(f"--- {lane['core']}/{lane['language']} {key} ---", file=sys.stderr)
                print(out, file=sys.stderr)
        results.append({
            "core": lane["core"],
            "language": lane["language"],
            "framework_id": lane.get("framework_id", ""),
            "dimensions": r["dims"],
            "required_dimensions": lane["required_dims"],
            "lifecycle_owners": {
                dimension: lifecycle_owner_for(lane, dimension)
                for dimension in lane["required_dims"]
            },
            # P0-6 (Tech Lead 2026-09-15): lanes are SPLIT by gate role —
            # Lifecycle evidence-only lanes stay visible with a source reason;
            # their native-PM claims, when declared, remain strictly gated.
            # (Lane lifecycle evidence-only vẫn hiện cùng lý do từ source;
            # claim native-PM nếu có vẫn bị gate nghiêm ngặt.)
            "evidence_only": bool(lane.get("evidence_only")),
            "release_scope_out_reason": release_scope_out_reason(
                lane["core"], lane["language"], lane.get("framework_id", "")
            ),
            "gate_role": (
                "scoped-out"
                if release_scope_out_reason(
                    lane["core"], lane["language"], lane.get("framework_id", "")
                )
                else "release-blocking"
            ),
            "toolchain_available": r.get("toolchain_available"),
            # Honest dimension rename (P0-mới-2): lanes whose install is
            # proven against a lane-injected fixture (not the template's
            # own dependency set) record that boundary in the dimension
            # NAME — the JSON can never be read as "template ships real
            # deps".
            # Đổi tên dimension trung thực (P0-mới-2): lane có install
            # được chứng minh trên fixture lane tự bơm (không phải
            # dependency set của template) ghi rõ biên đó trong TÊN
            # dimension — JSON không thể bị đọc là "template ship deps thật".
            "install_dim_name": lane.get("install_dim_name"),
            "native_pm_delegated": lane.get("native_pm_delegated", []),
            # Install OWNER (P0-D taxonomy, vòng-9): native-engine |
            # managed-delegation | plain-delegation — the JSON names who
            # owns resolve+lock+fetch+materialize so no verdict or doc
            # can launder delegation into a native claim.
            # (Chủ sở hữu install (taxonomy P0-D): native-engine |
            # managed-delegation | plain-delegation — JSON nêu rõ ai giữ
            # resolve+lock+fetch+materialize để không verdict hay tài
            # liệu nào giặt ủy quyền thành claim native.)
            "install_owner": lane.get(
                "install_owner",
                "plain-delegation" if "install" in lane.get("delegated", []) else "native-engine",
            ),
            # P0-D: who owns the dependency lifecycle for this lane — the
            # verdict gate below reads this field (native-pm-supported
            # REQUIRES mgc-native; delegated caps at compatibility-passed).
            # Emitted only after validate_dependency_owners() passed.
            # (P0-D: ai giữ lifecycle dependency của lane — cổng verdict
            # bên dưới đọc trường này (native-pm-supported BẮT BUỘC
            # mgc-native; delegated trần compatibility-passed). Chỉ emit
            # sau khi validate_dependency_owners() pass.)
            "dependency_owner": lane.get("dependency_owner"),
            # Per-OPERATION owners (Gate 11-C, vòng-11 verdict): resolve/
            # lock/fetch/store/materialize each name their owner — the
            # machine-readable split so "install passed" can never be
            # read as "resolve/lock/store native". Emitted only after
            # validate_lane_owners() passed (the gate ran at main() head).
            # (Owner THEO-TỪNG-OPERATION: resolve/lock/fetch/store/
            # materialize mỗi operation nêu owner — tách máy-đọc-được để
            # "install passed" không bao giờ bị đọc là "resolve/lock/
            # store native". Chỉ emit sau validate_lane_owners() pass
            # (cổng chạy ở đầu main()).)
            "owner_by_operation": lane.get("owner_by_operation", {}),
            "commit": commit,
            "verified_at": now,
            "detail": {
                k: v for k, v in r["detail"].items() if k.endswith("_output")
            },
        })

    # Verdicts are SPLIT by claim ownership (P1-mới + P1-D, Tech Lead
    # vòng-6/7): one label can no longer cover two different claims.
    # - `orchestration-lifecycle-passed` (P0-D rename, adversarial review
    #   vòng-9): EVERY required dimension is satisfied by `passed` OR
    #   `delegated` — mgc orchestrates the lane end-to-end, regardless of
    #   who owns the cache. The bare word "supported" invited marketing
    #   to read delegation as full native support; the new name says
    #   exactly what the evidence proves: the ORCHESTRATION passed.
    # - `native-pm-supported`: every dependency-engine dimension in
    #   NATIVE_PM_REQUIRED_DIMENSIONS is native-pass, operation owners are
    #   MGC/CAS, and no package manager is delegated. General create/test/
    #   build passes alone cannot establish this claim.
    # A lane with any delegated install can still be an orchestrator but
    # is NEVER native-pm — the two claims gate different marketing and
    # different release gates.
    # Verdict TÁCH theo quyền sở hữu claim: một nhãn không thể phủ hai
    # claim khác nhau. `orchestration-lifecycle-passed` (đổi tên P0-D
    # vòng-9): MỌI dimension required thỏa bởi `passed` HOẶC `delegated`
    # — mgc điều phối lane trọn vẹn, bất kể ai giữ cache; từ "supported"
    # trần từng mời marketing đọc delegation thành native-support đầy
    # đủ, tên mới nói đúng cái bằng chứng chứng minh: ORCHESTRATION
    # pass. `native-pm-supported`: mọi dependency-engine dimension trong
    # NATIVE_PM_REQUIRED_DIMENSIONS phải native-pass, owner từng operation
    # là MGC/CAS, và không delegated package manager. Pass create/test/build
    # đơn thuần không chứng minh claim này. Đây là verdict DUY NHẤT đủ
    # chứng minh claim "native multi-language package manager". Lane có install delegated vẫn là
    # orchestrator nhưng KHÔNG BAO GIỜ native-pm — hai claim gate khác
    # nhau về marketing lẫn release.
    def _satisfied(status):
        return status in PASS_STATUSES

    for r in results:
        dims = r["dimensions"]
        required = r.pop("required_dimensions")
        r["required_dimensions"] = required
        native_pm_delegated = r.get("native_pm_delegated", [])
        install_owner = r.get("install_owner", "plain-delegation")
        # P0-6: an evidence-only lane whose external toolchain is absent
        # gets the honest `evidence-unverified` verdict — it neither passes
        # nor fails, it records an unprobed environment (never a gate).
        # (P0-6: lane evidence-only thiếu toolchain ngoài nhận verdict
        # trung thực `evidence-unverified` — không pass, không fail, ghi
        # môi trường chưa probe (không bao giờ gate).)
        if not r.get("toolchain_available", True):
            r["verdict"] = "evidence-unverified"
            r["native_pm_verdict"] = native_pm_unavailable_verdict(
                r.get("dependency_owner")
            )
            continue
        r["verdict"] = (
            "orchestration-lifecycle-passed"
            if required and all(_satisfied(dims[d]) for d in required)
            else "partial" if any(_satisfied(dims[d]) for d in required)
            else "unsupported"
        )
        # Native-PM verdict (P0-D gate, 2026-09-16): `native-pm-supported`
        # ONLY when the lane's dependency_owner is `mgc-native` — a
        # delegated toolchain can never back a native claim no matter how
        # the dimensions scored. Delegated lanes cap at the NEW
        # `compatibility-passed` verdict (toolchain compatibility proven,
        # ownership NOT mgc's); `scaffold-only`/`unsupported` lanes read
        # `unsupported`. The install-OWNER taxonomy (P0-D vòng-9) and the
        # Native dependency dimensions and per-operation ownership are also
        # required; broad lifecycle requirements alone are insufficient.
        # (Verdict native-PM (cổng P0-D): `native-pm-supported` CHỈ khi
        # dependency_owner của lane là `mgc-native` — toolchain ủy quyền
        # không bao giờ đủ cho claim native dù dimension điểm thế nào.
        # Lane delegated trần ở verdict MỚI `compatibility-passed` (chứng
        # minh tương thích toolchain, KHÔNG phải sở hữu của mgc);
        # `scaffold-only`/`unsupported` đọc `unsupported`; các chiều
        # dependency-engine và owner từng operation cũng phải được chứng minh.)
        dep_owner = r.get("dependency_owner", "")
        r["native_pm_verdict"] = (
            "native-pm-supported"
            if native_pm_supported(
                dep_owner,
                install_owner,
                native_pm_delegated,
                dims,
                r.get("owner_by_operation", {}),
            )
            else "compatibility-passed"
            if dep_owner == "delegated"
            and required
            and all(_satisfied(dims[d]) for d in required)
            else "unsupported"
            if dep_owner in ("scaffold-only", "unsupported")
            else "not-native-pm"
        )

    out = {
        "schema_version": SCHEMA_VERSION,
        "generated_at": now,
        "commit": commit,
        "working_tree_clean": lifecycle_source_matches(
            commit, source_tree_clean_before_run
        ),
        "platform": lifecycle_platform_name(),
        "matrix_kind": "lifecycle",
        "release_scope": [list(item) for item in sorted(PLATFORM_EVIDENCE_RELEASE_SCOPE)],
        "framework_catalog": FRAMEWORK_QUALIFICATION_EVIDENCE,
        # The six-value status vocabulary (Gate 11-C schema v7) so every
        # consumer interprets statuses identically — only *-pass statuses
        # satisfy, unverified/unsupported/failed satisfy nothing.
        # (Bộ 6 trạng thái (Gate 11-C schema v7) để mọi consumer đọc
        # trạng thái thống nhất — chỉ trạng thái *-pass thỏa,
        # unverified/unsupported/failed không thỏa gì.)
        "status_vocabulary": list(PASS_STATUSES) + [
            STATUS_UNVERIFIED, STATUS_UNSUPPORTED, STATUS_FAILED,
        ],
        "dimensions": ALL_DIMENSIONS,
        # P0-3: per-OS CONSECUTIVE green-run platform evidence (see the
        # counter block above). Embedded so one artifact carries both the
        # matrix verdicts and how much platform evidence backs them.
        # (P0-3: evidence platform số lần xanh LIÊN TIẾP theo OS (xem block
        # counter bên trên). Nhúng vào đây để một artifact mang cả verdict
        # matrix lẫn lượng evidence platform đứng sau verdict đó.)
        "platform_evidence": _platform_evidence_load(),
        "lanes": results,
    }

    target = os.environ.get("MGC_LIFECYCLE_MATRIX_OUT", "docs/specs/lifecycleCapabilityMatrix.json")
    # CI checkouts do not carry the gitignored docs/specs tree — create
    # the parent directory before writing.
    # Checkout CI không có cây docs/specs (bị gitignored) — tạo thư mục
    # cha trước khi ghi.
    os.makedirs(os.path.dirname(target) or ".", exist_ok=True)
    with open(target, "w") as f:
        json.dump(out, f, indent=2)
        f.write("\n")
    print(f"lifecycle capability matrix written to {target} ({len(results)} lanes)")
    for r in results:
        passed = [d for d, s in r["dimensions"].items() if s in PASS_STATUSES]
        rest = {
            d: s for d, s in r["dimensions"].items() if s not in PASS_STATUSES
        }
        print(f"  {r['verdict']:<30} {r['core']}/{r['language']}: pass={passed}")
        print(f"      native_pm={r.get('native_pm_verdict')}")
        print(f"      non-pass={rest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
