#!/usr/bin/env python3
"""Generate the LIFECYCLE capability matrix from evidence — separate
from the audit capability matrix (Tech Lead P0-2, 2026-09-11).

The audit matrix (audit_capability_matrix.py) proves ADVISORY evidence
per lane (clean/vulnerable/tool-failure). It can NEVER justify a
"language supported" claim about the PRODUCT — lifecycle support is a
separate dimension with its own evidence: create → detect → resolve →
lock → fetch → install(materialize) → test → build → run → dev → audit
→ store → offline → optimizer → recovery.

Schema v2 (Gate 11-C, 2026-09-15): 16 dimensions, each recorded in a
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
evidence riêng: create → detect → resolve → lock → fetch →
install(materialize) → test → build → run → dev → audit → store →
offline → optimizer → recovery.

Schema v2 (Gate 11-C): 16 dimension, mỗi dimension ghi bằng MỘT TRONG
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
import re
import shutil
import subprocess
import sys
import tempfile

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
            "store": "magicore-shared-cas",  # blake3 CAS import
            "materialize": "mgc",            # mgc writes registry/{cache,src} layout
        },
    },
    {
        "core": "lib",
        "language": "python",
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
            "store": "magicore-shared-cas",  # blake3 CAS import
            "materialize": "mgc",            # mgc writes pypi/wheels layout
        },
    },
    {
        "core": "lib",
        "language": "typescript",
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
        # P0-D: the web engine IS mgc — lib/typescript is mgc-native.
        # (P0-D: engine web CHÍNH LÀ mgc — lib/typescript là mgc-native.)
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # web engine resolves the graph
            "lock": "mgc",                   # mgc.lock
            "fetch": "mgc",                  # mgc fetcher + CAS
            "store": "magicore-shared-cas",  # global shared CAS
            "materialize": "mgc",            # mgc materializes node_modules
        },
    },
    {
        "core": "lib",
        "language": "go",
        "scaffold": ["create-lib", "go", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
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
            "store": "magicore-shared-cas",  # blake3 CAS import
            "materialize": "mgc",            # GOMODCACHE layout for offline builds
        },
    },
    {
        "core": "lib",
        "language": "java",
        "scaffold": ["create-lib", "java", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
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
            "store": "magicore-shared-cas",  # sha256/sha1-verified CAS import
            "materialize": "mgc",            # M2 repository layout for offline builds
        },
    },
    {
        "core": "lib",
        "language": "dotnet",
        "scaffold": ["create-lib", "dotnet", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
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
        # P0-D: the web engine IS mgc — the flagship mgc-native lane.
        # (P0-D: engine web CHÍNH LÀ mgc — lane mgc-native chủ lực.)
        "dependency_owner": "mgc-native",
        "owner_by_operation": {
            "resolve": "mgc",                # web engine resolves the graph
            "lock": "mgc",                   # mgc.lock
            "fetch": "mgc",                  # mgc fetcher + CAS
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
        "core": "ai",
        "language": "python",
        "scaffold": ["create-ai", "python-agent", "test-ai"],
        # mgc install (ai) is FAIL-CLOSED: it requires uv.lock or
        # requirements.lock (05 §5). The lane locks with uv FIRST (the
        # provisioned tool the workflow installs), then runs the real
        # install — no skip, no environment-unverified marker.
        # mgc install (ai) FAIL-CLOSED: cần uv.lock hoặc
        # requirements.lock (05 §5). Lane lock bằng uv TRƯỚC (tool được
        # workflow provision), rồi chạy install thật — không skip,
        # không marker environment-unverified.
        "pre_steps": [
            "lock_with_uv",
            # REAL dependency fixture (P0-mới-2, Tech Lead vòng-6/7): the
            # template ships deps = [], so a bare `uv lock` + `uv sync`
            # locks and installs ZERO packages — an exit-0 that proves
            # NOTHING about installing a real dependency graph. The lane
            # now adds a real direct dependency (markerlib) via `uv add`
            # BEFORE locking, so the lockfile carries a real resolve +
            # hash and `mgc install` must actually materialize it.
            # Fixture dependency THẬT (P0-mới-2): template ship deps = []
            # nên `uv lock` + `uv sync` trần lock + cài KHÔNG package nào
            # — exit-0 không chứng minh gì về cài dependency graph thật.
            # Lane giờ thêm dependency trực tiếp thật (markerlib) qua
            # `uv add` TRƯỚC khi lock, để lockfile mang resolve + hash
            # thật và `mgc install` phải materialize nó.
            "uv_add_real_dependency",
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
        # install dimension is proven against a lane-injected fixture,
        # so the honest dimension name records that boundary
        # (install-command-empty-project until the template itself ships
        # real deps — Tech Lead P0-mới-2).
        # Lane AI: scaffold không kèm dependency — dimension install
        # được chứng minh trên fixture lane tự thêm, nên tên dimension
        # trung thực ghi rõ biên đó (install-command-empty-project tới
        # khi template tự ship dependency thật — P0-mới-2).
        "required_dims": ["create", "install", "test", "build"],
        "install_dim_name": "install-command-empty-project",
        "native_pm_delegated": ["install"],
        # uv locks + resolves + installs (the lane pre-locks with uv) —
        # managed delegation, never native (P0-D).
        # (uv lock + resolve + install (lane lock trước bằng uv) — ủy
        # quyền có quản lý, không bao giờ native (P0-D).)
        "install_owner": "managed-delegation",
        # P0-D: uv owns the ai/python dependency lifecycle — delegated.
        # (P0-D: uv giữ lifecycle dependency của ai/python — delegated.)
        "dependency_owner": "delegated",
        "owner_by_operation": {
            "resolve": "uv",
            "lock": "uv",
            "fetch": "uv",
            "store": "magicore-managed-uv-cache",
            "materialize": "uv",
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
        # C0 (T0.3, 2026-09-17): the invoked install lane runs
        # `flutter pub get` for real — delegated, never mgc-native, even
        # though a native pub.dev resolve engine exists (unwired to the
        # lane; Phase C). The C0 firewall refuses this lane without an
        # explicit --compat-runtime opt-in.
        # (C0: lane install gọi `flutter pub get` thật — delegated, không
        # bao giờ mgc-native, dù engine resolve pub.dev native tồn tại
        # (chưa nối vào lane; Phase C).)
        "delegated": [],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "plain-delegation",
        # P0-D/T0.4: flutter owns the app/flutter dependency lifecycle
        # (lane spawns `flutter pub get`; C0-gated).
        # (P0-D/T0.4: flutter giữ lifecycle dependency app/flutter.)
        "dependency_owner": "delegated",
        "note": "lane delegates to `flutter pub get` (C0-gated); native pub.dev resolve engine exists but is unwired to the lane (Phase C)",
        "owner_by_operation": {
            "resolve": "flutter",            # lane runs `flutter pub get`
            "lock": "flutter",               # pubspec.lock owned by flutter
            "fetch": "flutter",              # flutter downloads the archive
            "store": "flutter-pub-cache",    # no shared mgc CAS on this lane
            "materialize": "flutter",        # flutter writes .dart_tool
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
        # C0 (T0.3, 2026-09-17): the invoked install lane runs
        # `swift package resolve` for real — delegated, never mgc-native,
        # even though a native SwiftPM registry engine exists for
        # registry packages (unwired to the lane; Phase C). Git-only
        # dependencies stay default-blocked.
        # (C0: lane install chạy `swift package resolve` thật —
        # delegated.)
        "install_owner": "plain-delegation",
        # P0-D/T0.4: swift owns the app/swift dependency lifecycle.
        "dependency_owner": "delegated",
        "note": "lane delegates to `swift package resolve` (C0-gated); native SwiftPM registry engine exists but is unwired to the lane (Phase C); git-only deps default-blocked",
        "owner_by_operation": {
            "resolve": "swift",
            "lock": "swift",
            "fetch": "swift",
            "store": "swift-package-cache",
            "materialize": "swift",
        },
    },
    {
        "core": "app",
        "language": "objc",
        "evidence_only": True,
        "toolchain_probes": ["pod"],
        "scaffold": ["create-app", "objc", "test-app"],
        "steps": [("install", ["install"])],
        "required_dims": ["create", "install"],
        # C0 (T0.3, 2026-09-17): the invoked install lane runs
        # `xcodebuild -resolvePackageDependencies` for real — delegated;
        # no ObjC-native resolve engine exists at all.
        # (C0: lane install chạy xcodebuild thật — delegated; không có
        # engine resolve ObjC native nào.)
        "install_owner": "plain-delegation",
        # P0-D/T0.4: xcodebuild owns the app/objc dependency lifecycle.
        "dependency_owner": "delegated",
        "note": "lane delegates to `xcodebuild -resolvePackageDependencies` (C0-gated); no ObjC-native engine exists",
        "owner_by_operation": {
            "resolve": "xcodebuild",
            "lock": "xcodebuild",
            "fetch": "xcodebuild",
            "store": "xcode-derived-data",
            "materialize": "xcodebuild",
        },
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
        "install_owner": "plain-delegation",
        # P0-D/T0.4: no install lifecycle exists on the invoked lane.
        "dependency_owner": "unsupported",
        "note": "per-tier: js=web-pipeline, android=Maven-engine, ios=CocoaPods-CDN-verify; invoked lane has no runner and errors (fail-closed); iOS tier fail-closed without Podfile.lock",
        "owner_by_operation": {
            "resolve": "mgc-per-tier",
            "lock": "unsupported-no-runner",
            "fetch": "unsupported-no-runner",
            "store": "unsupported-no-runner",
            "materialize": "unsupported-no-runner",
        },
    },
    # ===== Evidence-only lanes (P0-6, Tech Lead 2026-09-15) =====
    # Five lanes RECORD evidence but NEVER gate a release. Their external
    # toolchains are often absent on dev machines and CI runners — when a
    # probe fails, EVERY dimension is recorded as `unverified` (honest
    # absence, never a pass, never a silent skip) and the verdict becomes
    # `evidence-unverified`. The JSON carries `"evidence_only": true` and
    # `"gate_role": "evidence-only"` so no consumer can mistake them for
    # release-blocking lanes. Owner taxonomy is STILL declared (the Gate
    # 11-C owner gate stays intact — evidence-only is not a metadata
    # bypass).
    # (Năm lane CHỈ-GHI-BẰNG-CHỨNG, KHÔNG bao giờ gate release. Toolchain
    # ngoài của chúng thường vắng trên máy dev và runner CI — probe hụt thì
    # MỌI dimension ghi `unverified` (vắng mặt trung thực, không phải pass,
    # không skip âm thầm) và verdict thành `evidence-unverified`. JSON mang
    # `"evidence_only": true` + `"gate_role": "evidence-only"` để không
    # consumer nào đọc nhầm thành lane chặn release. Taxonomy owner VẪN
    # khai (cổng owner Gate 11-C giữ nguyên — evidence-only không phải
    # lối thoát khỏi metadata).)
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
        "install_owner": "plain-delegation",
        # P0-D: bevy-rust delegates the dependency lifecycle to cargo
        # (`cargo fetch` runs for real; mgc only orchestrates).
        # (P0-D: bevy-rust ủy quyền lifecycle dependency cho cargo
        # (`cargo fetch` chạy thật; mgc chỉ điều phối).)
        "dependency_owner": "delegated",
        "owner_by_operation": {
            "resolve": "cargo",
            "lock": "cargo",
            "fetch": "cargo",
            "store": "cargo-target-cache",
            "materialize": "cargo",
        },
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
        "install_owner": "plain-delegation",
        # P0-D: the embedded toolchains own the lifecycle (cargo fetch /
        # pio pkg install / west update) — delegated.
        # (P0-D: toolchain embedded giữ lifecycle (cargo fetch / pio pkg
        # install / west update) — delegated.)
        "dependency_owner": "delegated",
        "owner_by_operation": {
            "resolve": "cargo",
            "lock": "cargo",
            "fetch": "cargo",
            "store": "cargo-target-cache",
            "materialize": "cargo",
        },
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
        "delegated": ["install"],
        "required_dims": ["create", "install", "test", "build"],
        "install_owner": "plain-delegation",
        # P0-D: terraform owns providers/modules (`init`+`get`) —
        # delegated (cloud passthrough, honest).
        # (P0-D: terraform giữ provider/module (`init`+`get`) — delegated.)
        "dependency_owner": "delegated",
        "owner_by_operation": {
            "resolve": "terraform",
            "lock": "terraform",           # .terraform.lock.hcl
            "fetch": "terraform",          # terraform init downloads providers
            "store": "terraform-provider-cache",
            "materialize": "terraform",
        },
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
        "install_owner": "managed-delegation",
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
        "owner_by_operation": {
            "resolve": "unsupported-no-registry-graph",   # adapter.rs:55 Unsupported
            "lock": "unsupported-no-lockfile",            # no lockfile surface
            "fetch": "unsupported-nothing-to-fetch",      # adapter.rs:67 Unsupported
            "store": "magicore-bundled-templates",        # templates ship inside mgc
            "materialize": "mgc-add-hardware-generator",  # add-hardware writes the tree
        },
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
        "install_owner": "plain-delegation",
        # P0-D: cicd is a generator lane (P0-5 — the adapter returns
        # Unsupported for the registry surface; pipelines are hand-owned
        # afterwards): scaffold-only.
        # (P0-D: cicd là lane generator (P0-5 — adapter trả Unsupported
        # cho mặt registry; pipeline do người quản sau đó): scaffold-only.)
        "dependency_owner": "scaffold-only",
        "owner_by_operation": {
            "resolve": "unsupported-no-registry-graph",  # adapter.rs:65 Unsupported
            "lock": "unsupported-no-lockfile",           # no lockfile surface
            "fetch": "unsupported-nothing-to-fetch",     # adapter.rs:75 Unsupported
            "store": "ci-provider-owned",                # provider owns caches/artifacts
            "materialize": "ci-provider-owned",          # workflows run on the provider
        },
    },
]

# Binary step timeout (seconds) — overridable via MGC_LIFECYCLE_STEP_TIMEOUT.
# Timeout mỗi bước binary — ghi đè qua MGC_LIFECYCLE_STEP_TIMEOUT.
STEP_TIMEOUT_DEFAULT_S = 600

# Schema v2 (Gate 11-C): the 16 lifecycle dimensions. The v1 names
# `cache-reuse`/`offline-reinstall` are renamed `store`/`offline` so the
# `.dimensions` list is the canonical 16-name contract — a consumer of the
# JSON can rely on the order and the names below.
# (Schema v2 (Gate 11-C): 16 dimension lifecycle. Tên v1 `cache-reuse`/
# `offline-reinstall` đổi thành `store`/`offline` để `.dimensions` là hợp
# đồng 16 tên chuẩn — consumer của JSON dựa được vào tên và thứ tự dưới.)
ALL_DIMENSIONS = ["create", "install", "test", "build", "run", "dev", "audit",
                  "store", "offline", "detect", "resolve", "lock", "fetch",
                  "materialize", "optimizer", "recovery"]

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

# JSON schema version (v2: 16 dimensions + status vocabulary).
# (Phiên bản schema JSON (v2: 16 dimension + bộ trạng thái).)
SCHEMA_VERSION = 2

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


def scaffold_language_matches(sandbox: str, project_dir: str, language: str) -> bool:
    """Verify the scaffolded project carries a manifest of the REQUESTED
    language (anti-fake-scaffold guard, P0-2 honesty contract).
    Xác minh project được sinh có manifest của ĐÚNG ngôn ngữ yêu cầu
    (chống scaffold giả — hợp đồng trung thực P0-2)."""
    markers = SCAFFOLD_MARKERS.get(language, [])
    if not markers:
        return True  # unknown language — cannot verify, do not guess
    root = os.path.join(sandbox, project_dir)
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
# Gate 11-C fine-grained evidence helpers (schema v2 dimensions: detect,
# resolve, lock, fetch, materialize, optimizer, recovery).
# (Hàm phụ trợ bằng chứng tinh-fine Gate 11-C cho các dimension schema v2.)
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
    if language in ("javascript", "typescript"):
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


def _materialize_marker(project_dir: str, language: str):
    """Return (label, path) whose non-emptiness PROVES the dependency tree
    landed after install, or (None, None) when this ecosystem has no
    observable marker — which records `unverified`, never a pass.
    (Trả về (nhãn, đường_dẫn) mà việc không-rỗng CHỨNG MINH cây dependency
    đã thật sự về sau install, hoặc (None, None) khi ecosystem không có
    marker quan sát được — ghi `unverified`, không bao giờ pass.)"""
    if language in ("javascript", "typescript"):
        return ("node_modules", os.path.join(project_dir, "node_modules"))
    if language == "python":
        return (".venv", os.path.join(project_dir, ".venv"))
    if language == "rust":
        # Rust deps never land in-project — cargo materializes them into
        # the mgc-managed CARGO_HOME (~/.magicore/store/cargo, see
        # adapters/lib/src/install/shared_store.rs). That managed store is
        # the materialization surface for this lane.
        # (Deps Rust không bao giờ về trong project — cargo materialize
        # vào CARGO_HOME do mgc quản (~/.magicore/store/cargo). Store quản
        # lý đó là mặt materialize của lane này.)
        home = os.path.expanduser("~")
        return ("managed-cargo-home", os.path.join(home, ".magicore", "store", "cargo"))
    if language == "go":
        # The go module cache lives at GOMODCACHE — observable only when
        # the go toolchain is on PATH; otherwise unverified.
        # (Module cache của go nằm ở GOMODCACHE — chỉ quan sát được khi go
        # có trên PATH; không thì unverified.)
        go = shutil.which("go")
        if go is None:
            return (None, None)
        try:
            proc = subprocess.run(
                [go, "env", "GOMODCACHE"], capture_output=True, text=True, timeout=30
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
    iso_cache = os.path.join(rec_project, ".magicore-recovery")
    env = os.environ.copy()
    env["MGC_CACHE_DIR"] = iso_cache

    def _doctor(*extra: str):
        proc = subprocess.run(
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
        baseline = subprocess.run(
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
REQUIRED_OWNER_OPERATIONS = ["resolve", "lock", "fetch", "store", "materialize"]


def validate_lane_owners() -> int:
    """Gate 11-C owner gate: every lane declares install_owner AND an
    owner for each install operation, and the per-operation owners agree
    with the coarse install_owner label (native-engine lanes must have
    mgc/magicore owning resolve, lock, fetch, store, materialize — any
    native-toolchain owner in that set contradicts the label and BLOCKS).
    Trả về 0 khi sạch; in lỗi và trả 1 khi có lane vi phạm.
    Cổng owner Gate 11-C: mọi lane khai install_owner VÀ owner cho từng
    operation install; owner per-operation phải khớp nhãn install_owner
    thô (lane native-engine phải có mgc/magicore giữ resolve, lock,
    fetch, store, materialize — owner toolchain-native trong tập đó mâu
    thuẫn với nhãn và BỊ CHẶN)."""
    violations = []
    for lane in LANES:
        tag = f"{lane['core']}/{lane['language']}"
        owner = lane.get("install_owner")
        if not owner:
            violations.append(f"{tag}: missing install_owner")
            continue
        if owner not in ("native-engine", "managed-delegation", "plain-delegation"):
            violations.append(f"{tag}: unknown install_owner '{owner}'")
        ops = lane.get("owner_by_operation", {})
        if not ops:
            violations.append(f"{tag}: missing owner_by_operation (Gate 11-C per-operation taxonomy)")
            continue
        for op in REQUIRED_OWNER_OPERATIONS:
            if not ops.get(op):
                violations.append(f"{tag}: owner_by_operation missing '{op}'")
        # Consistency: a native-engine lane must have mgc/magicore owning
        # EVERY operation — a native toolchain owner anywhere contradicts
        # the native-engine label.
        # (Nhất quán: lane native-engine phải có mgc/magicore giữ MỌI
        # operation — owner toolchain-native ở bất kỳ đâu mâu thuẫn với
        # nhãn native-engine.)
        if owner == "native-engine":
            for op in REQUIRED_OWNER_OPERATIONS:
                op_owner = ops.get(op, "")
                if op_owner and not (
                    op_owner.startswith("mgc") or op_owner.startswith("magicore")
                ):
                    violations.append(
                        f"{tag}: install_owner=native-engine but '{op}' is owned by "
                        f"'{op_owner}' — contradiction (install pass cannot launder "
                        f"into native-{op} pass)"
                    )
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
#   game  → resolve/fetch FALSE: MgError::Unsupported, "managed by the
#           engine's own toolchain" (adapters/game/src/adapter.rs:58/70);
#           install TRUE via engine exec only (bevy → `cargo fetch`,
#           adapter.rs:78; Godot/Unity/Unreal fail-closed).
#   iot   → resolve/fetch FALSE: MgError::Unsupported —
#           adapters/iot/src/adapter.rs:58/70; install TRUE via toolchain
#           exec (cargo/pio/west, adapter.rs:80).
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
# (Bảng capability production cứng từ adapter THẬT — taxonomy lane phải
# khớp bảng này, không khớp khát vọng: lane nào adapter trả Unsupported
# cho install/resolve/fetch thì cấm native-engine và cấm owner mgc cho
# op đó — đúng false-positive P0-5 của hardware/cicd.)
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


def validate_adapter_consistency() -> int:
    """P0-5 consistency gate (Tech Lead verdict 2026-09-16): the lane
    taxonomy must match the PRODUCTION adapters' real capabilities — an
    adapter that returns MgError::Unsupported for install/resolve/fetch
    can never back a `native-engine` label or an mgc-owned per-operation
    claim. Fail-closed: any mismatch (or an unknown core row) blocks with
    exit 1.
    Cổng nhất quán P0-5: taxonomy lane phải khớp capability THẬT của
    adapter production — adapter trả MgError::Unsupported cho
    install/resolve/fetch thì không bao giờ được đứng sau nhãn
    native-engine hay claim owner mgc. Fail-closed: lệch (hoặc core chưa
    có dòng bảng) là chặn exit 1."""
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
        f"adapter consistency gate: {len(LANES)} lanes match "
        "production adapter capabilities (P0-5)"
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
    """Pure cross-check: matrix lane `dependency_owner` vs the binary's
    `mgc capabilities --json` dependency_ownership table (install op).
    Both directions fail — a lane claiming native the binary denies is
    laundering, and a lane denying native the binary proves is stale.
    `unsupported` on the binary side accepts matrix `unsupported` or
    `scaffold-only` (both mean "no supported install lifecycle").
    (Đối chiếu thuần: `dependency_owner` của lane matrix với bảng của
    binary. Cả hai chiều đều fail.)
    `binary_ownership`: {core: {"install": owner, "languages": {lang:
    owner}}} with owners mgc-native | delegated | unsupported. Matrix
    lane languages map to gate languages (typescript->ts,
    react-native->rn, others identical).
    Returns the violation list (empty = consistent)."""
    expected = {
        "mgc-native": ("mgc-native",),
        "delegated": ("delegated",),
        "unsupported": ("unsupported", "scaffold-only"),
    }
    language_alias = {"typescript": "ts", "react-native": "rn"}
    violations = []
    for lane in lanes:
        tag = f"{lane['core']}/{lane['language']}"
        claimed = lane.get("dependency_owner")
        entry = binary_ownership.get(lane["core"])
        if entry is None:
            violations.append(f"{tag}: core missing from binary ownership table")
            continue
        gate_lang = language_alias.get(lane["language"], lane["language"])
        truth = entry.get("languages", {}).get(gate_lang)
        if truth is None:
            truth = entry.get("install")
        if truth is None:
            violations.append(f"{tag}: install ownership missing from binary table")
            continue
        if claimed not in expected.get(truth, ()):
            violations.append(
                f"{tag}: matrix claims '{claimed}' but the binary owns "
                f"install as '{truth}'"
            )
    return violations


def validate_dep_gate_consistency(mgc_bin=None) -> int:
    """T0.4 binary↔matrix consistency gate: shell `mgc capabilities` and
    require every lane's `dependency_owner` to agree with the C0 firewall
    table for the install op. No binary → honest UNAVAILABLE record
    (internal taxonomy gates above still enforced; release-gating runs
    MUST provide the binary).
    (Cổng nhất quán T0.4: gọi `mgc capabilities` và bắt mọi lane khớp
    bảng tường lửa C0 ở op install. Không có binary → ghi UNAVAILABLE
    trung thực.)"""
    import subprocess

    candidates = []
    if mgc_bin:
        candidates.append(mgc_bin)
    env_bin = os.environ.get("MGC_BIN")
    if env_bin:
        candidates.append(env_bin)
    candidates.append("./target/debug/mgc")
    binary = next((c for c in candidates if os.path.isfile(c)), None)
    if binary is None:
        print(
            "dep-gate cross-check: UNAVAILABLE (no mgc binary) — "
            "internal taxonomy gates still enforced; release runs must "
            "provide the binary"
        )
        return 0
    try:
        raw = subprocess.run(
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
    try:
        payload = json.loads(raw.stdout)
        ownership = {}
        for core in payload["cores"]:
            table = core["dependency_ownership"]
            entry = {
                "install": table["operations"]["install"]["owner"],
                "languages": {
                    language: cells["install"]["owner"]
                    for language, cells in table.get("languages", {}).items()
                },
            }
            ownership[core["core"]] = entry
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
    print(f"dep-gate cross-check: {len(LANES)} lanes agree with the binary")
    return 0


def print_dependency_owner_summary() -> None:
    """Validate-only report: per-lane dependency_owner table with the
    verdict ceiling it imposes, plus a machine-readable JSON block on
    stdout so a consumer never parses the human table.
    (Báo cáo validate-only: bảng dependency_owner từng lane kèm trần
    verdict nó áp, cộng khối JSON máy-đọc trên stdout để consumer không
    bao giờ phải parse bảng người-đọc.)"""
    ceiling = {
        "mgc-native": "native-pm-supported (ceiling)",
        "delegated": "compatibility-passed (ceiling)",
        "scaffold-only": "unsupported",
        "unsupported": "unsupported",
    }
    print("=== dependency_owner summary (P0-D) ===")
    print(
        f"  {'lane':<24} {'dependency_owner':<16} "
        f"{'install_owner':<20} verdict ceiling"
    )
    for lane in LANES:
        tag = f"{lane['core']}/{lane['language']}"
        print(
            f"  {tag:<24} {lane['dependency_owner']:<16} "
            f"{lane.get('install_owner', '-'):<20} "
            f"{ceiling[lane['dependency_owner']]}"
        )
    print(
        json.dumps(
            {
                "dependency_owner_gate": "pass",
                "lanes": [
                    {
                        "core": lane["core"],
                        "language": lane["language"],
                        "dependency_owner": lane["dependency_owner"],
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
    evidence = _platform_evidence_load()
    entry = evidence.get(os_name)
    if not isinstance(entry, dict):
        entry = {}
    entry["runs"] = int(entry.get("runs", 0)) + 1
    sha = (os.environ.get("MGC_PLATFORM_EVIDENCE_SHA") or "").strip()
    if not sha:
        proc = subprocess.run(
            ["git", "rev-parse", "HEAD"], capture_output=True, text=True
        )
        sha = (proc.stdout or "").strip()
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


def run_lane(mgc_bin: str, lane: dict) -> dict:
    """Execute one lane's lifecycle steps in a fresh sandbox; record
    per-dimension status: passed / delegated / failed / absent.
    Chạy các bước lifecycle của một lane trong sandbox mới; ghi trạng
    thái từng dimension: passed / delegated / failed / absent."""
    timeout_s = int(os.environ.get("MGC_LIFECYCLE_STEP_TIMEOUT", STEP_TIMEOUT_DEFAULT_S))
    sandbox = tempfile.mkdtemp(prefix=f"mgc-lc-{lane['core']}-{lane['language']}-")
    dims: dict[str, str] = {}
    detail = {"sandbox": sandbox}

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

    def run_step(argv: list[str], subdir: str = "") -> tuple[int, str]:
        # Steps run INSIDE the scaffolded project (cwd = sandbox/<name>)
        # — install/test/build belong to the project, not the sandbox
        # root where two projects could otherwise collide.
        # Bước chạy BÊN TRONG project (cwd = sandbox/<name>) —
        # install/test/build thuộc project, không thuộc root sandbox.
        cwd = os.path.join(sandbox, subdir) if subdir else sandbox
        try:
            proc = subprocess.run(
                [mgc_bin] + argv, capture_output=True, text=True,
                cwd=cwd, timeout=timeout_s,
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
    project_dir = lane["scaffold"][-1]
    if rc != 0:
        dims["create"] = STATUS_FAILED
        # The scaffold never produced a project — detect was never
        # exercised: unverified, never a pass (fail-closed).
        # (Scaffold chưa sinh được project — detect chưa từng chạy:
        # unverified, không bao giờ pass (fail-closed).)
        dims["detect"] = STATUS_UNVERIFIED
        detail["create_output"] = out[-2000:]
    elif not scaffold_language_matches(sandbox, project_dir, lane["language"]):
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

    for step in lane.get("pre_steps", []):
        # Non-mgc preparation (e.g. `uv lock` for the fail-closed ai
        # install): runs the tool named here INSIDE the project BEFORE
        # the lifecycle steps. A failing prep is a FAILED lane prep —
        # recorded, never skipped (the workflow provisions the tool,
        # so a missing tool is a provisioning bug, not a pass).
        # Chuẩn bị ngoài mgc (vd `uv lock` cho install ai fail-closed):
        # chạy tool trong project TRƯỚC các bước lifecycle. Prep fail là
        # lane prep FAILED — ghi lại, không skip (workflow provision
        # tool nên thiếu tool là lỗi provision, không phải pass).
        if dims.get("create") != STATUS_NATIVE:
            break
        if step == "lock_with_uv":
            try:
                proc = subprocess.run(
                    ["uv", "lock"], capture_output=True, text=True,
                    cwd=os.path.join(sandbox, project_dir), timeout=timeout_s,
                )
            except FileNotFoundError:
                _fail("pre_step 'lock_with_uv' requires uv on PATH — provision it first")
            except subprocess.TimeoutExpired:
                _fail(f"uv lock prep timed out after {timeout_s}s")
            if proc.returncode != 0:
                dims["install"] = "failed"
                detail["install_output"] = (
                    "uv lock prep failed: "
                    + (proc.stdout or "") + (proc.stderr or "")
                )[-2000:]
        elif step == "uv_add_real_dependency":
            # P0-mới-2: inject a REAL direct dependency BEFORE locking so
            # the lockfile carries a genuine resolve + hash and `uv sync`
            # inside `mgc install` must materialize a real package — an
            # empty deps=[] lock proves nothing. `uv add` also runs a
            # resolve itself, so a failure here is a provisioning/network
            # problem recorded honestly.
            # (P0-mới-2: bơm dependency trực tiếp THẬT trước khi lock để
            # lockfile mang resolve + hash thật và `uv sync` bên trong
            # `mgc install` phải materialize package thật — lock deps=[]
            # rỗng không chứng minh gì. `uv add` tự chạy resolve, nên fail
            # ở đây là lỗi provision/network, ghi trung thực.)
            try:
                proc = subprocess.run(
                    ["uv", "add", "markerlib"], capture_output=True, text=True,
                    cwd=os.path.join(sandbox, project_dir), timeout=timeout_s,
                )
            except FileNotFoundError:
                _fail("pre_step 'uv_add_real_dependency' requires uv on PATH — provision it first")
            except subprocess.TimeoutExpired:
                _fail(f"uv add prep timed out after {timeout_s}s")
            if proc.returncode != 0:
                dims["install"] = "failed"
                detail["install_output"] = (
                    "uv add (real dependency fixture) failed: "
                    + (proc.stdout or "") + (proc.stderr or "")
                )[-2000:]
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
            launcher = "python3"
            if shutil.which("python") is not None:
                launcher = "python"
            try:
                proc = subprocess.run(
                    [launcher, "-m", "pip", "install", "-q", "build", "pytest"],
                    capture_output=True, text=True,
                    cwd=os.path.join(sandbox, project_dir), timeout=timeout_s,
                )
            except FileNotFoundError:
                _fail("pre_step 'provision_py_build_tools' requires a python launcher on PATH")
            except subprocess.TimeoutExpired:
                _fail(f"py build tools install timed out after {timeout_s}s")
            if proc.returncode != 0:
                dims["build"] = "failed"
                dims["test"] = "failed"
                detail["build_output"] = (
                    "provision_py_build_tools failed: "
                    + (proc.stdout or "") + (proc.stderr or "")
                )[-2000:]
        else:
            _fail(f"unknown pre_step: {step}")

    for step, argv in lane["steps"]:
        if dims.get("create") != STATUS_NATIVE:
            # The lane broke before this step: SKIPPED, never a pass —
            # a skip records `unverified`, which satisfies nothing
            # (fail-closed, schema v2).
            # (Lane vỡ trước bước này: SKIP, không bao giờ là pass — skip
            # ghi `unverified`, không thỏa điều gì (fail-closed, schema v2).)
            dims[step] = STATUS_UNVERIFIED
            continue
        rc, out = run_step(argv, subdir=project_dir)
        if rc != 0:
            dims[step] = STATUS_FAILED
            detail[f"{step}_output"] = out[-2000:]
        elif step == "install" or step in lane["delegated"]:
            # Success follows the OWNER taxonomy: an install (or delegated
            # step) success records exactly who owns it — native-engine
            # lanes record native-pass, managed stays managed, plain stays
            # plain. "Install passed" can never be read as more native
            # than the lane's declared ownership (Gate 11-C).
            # (Thành công theo taxonomy CHỦ SỞ HỮU: install (hoặc bước
            # delegated) thành công ghi đúng owner — lane native-engine
            # ghi native-pass, managed vẫn managed, plain vẫn plain.
            # "Install passed" không bao giờ được đọc là native hơn quyền
            # sở hữu lane khai (Gate 11-C).)
            dims[step] = _owner_pass_status(lane)
        else:
            # Non-install steps (test/build) run under mgc's own
            # orchestration → native-pass.
            # (Bước không-install (test/build) chạy dưới điều phối của
            # chính mgc → native-pass.)
            dims[step] = STATUS_NATIVE

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
                subprocess.run(
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
                    out = subprocess.run(
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
            dims["run"] = STATUS_NATIVE if served else STATUS_FAILED
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
                subprocess.run(
                    ["taskkill", "/T", "/F", "/PID", str(dev_proc.pid)],
                    capture_output=True,
                )
                dev_proc.terminate()

        def _dev_sweep_port():
            if os.name != "nt":
                try:
                    out = subprocess.run(
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
                dims["dev"] = STATUS_NATIVE
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
        label, marker = _materialize_marker(project_path, lane["language"])
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
    # cli/tests/kill_injection_matrix.rs). Only lanes whose install runs
    # the mgc web orchestrator (install_owner=native-engine — web/js, and
    # lib/ts which delegates to web.install) HAVE this crash surface;
    # other lanes are honestly unsupported. POSIX only: no signal support
    # → unverified, never faked.
    # (recovery — dimension QUAN TRỌNG NHẤT (Gate 11-C): crash thật + sửa
    # chữa thật qua handshake failpoint có sẵn. `mgc install` đỗ tại
    # after-generation-begin (marker READY do binary fsync), harness
    # SIGKILL, rồi `mgc store doctor --repair` phải để store HEALTHY với
    # 0 stale staging (hợp đồng Rust: cli/tests/kill_injection_matrix.rs).
    # Chỉ lane có install chạy orchestrator web của mgc (install_owner=
    # native-engine — web/js, và lib/ts delegate sang web.install) CÓ mặt
    # crash này; lane khác là unsupported trung thực. Chỉ POSIX: không
    # hỗ trợ signal → unverified, không bao giờ bịa.)
    if lane.get("install_owner") != "native-engine":
        dims["recovery"] = STATUS_UNSUPPORTED
        detail["recovery_output"] = (
            "failpoint crash surface exists only in the mgc web install "
            "orchestrator (native-engine lanes)"
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
    if cli_args:
        if cli_args[0] in ("--record-green", "--reset") and len(cli_args) == 2:
            if cli_args[0] == "--record-green":
                return record_platform_green(cli_args[1])
            return reset_platform_counter(cli_args[1])
        _fail(
            f"unknown arguments: {' '.join(cli_args)} "
            "(expected --record-green <os> / --reset <os>)"
        )
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
    if (
        validate_lane_owners() != 0
        or validate_adapter_consistency() != 0
        or validate_dependency_owners() != 0
    ):
        _fail(
            "lane owner/adapter-consistency/dependency-owner gates failed — "
            "refusing to collect a matrix from mislabeled taxonomy"
        )

    commit = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
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
            "dimensions": r["dims"],
            "required_dimensions": lane["required_dims"],
            # P0-6 (Tech Lead 2026-09-15): lanes are SPLIT by gate role —
            # `release-blocking` lanes gate the RC, `evidence-only` lanes
            # record what the ecosystem does today and NEVER gate. The
            # machine-readable split means a consumer never has to guess
            # from lane names.
            # (P0-6: lane TÁCH theo vai trò gate — lane `release-blocking`
            # gate RC, lane `evidence-only` chỉ ghi hệ sinh thái hôm nay và
            # KHÔNG BAO GIỜ gate. Tách máy-đọc-được để consumer không phải
            # đoán từ tên lane.)
            "evidence_only": bool(lane.get("evidence_only")),
            "gate_role": "evidence-only" if lane.get("evidence_only") else "release-blocking",
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
    # - `native-pm-supported`: every required dimension is `passed` AND
    #   the lane's install owner is `native-engine` (mgc ITSELF owns the
    #   install — shared store, CAS, lockfile), no dimension rides a
    #   native toolchain. This is the ONLY verdict that can back a
    #   "native multi-language package manager" claim.
    # A lane with any delegated install can still be an orchestrator but
    # is NEVER native-pm — the two claims gate different marketing and
    # different release gates.
    # Verdict TÁCH theo quyền sở hữu claim: một nhãn không thể phủ hai
    # claim khác nhau. `orchestration-lifecycle-passed` (đổi tên P0-D
    # vòng-9): MỌI dimension required thỏa bởi `passed` HOẶC `delegated`
    # — mgc điều phối lane trọn vẹn, bất kể ai giữ cache; từ "supported"
    # trần từng mời marketing đọc delegation thành native-support đầy
    # đủ, tên mới nói đúng cái bằng chứng chứng minh: ORCHESTRATION
    # pass. `native-pm-supported`: mọi dimension required là `passed`
    # VÀ chủ sở hữu install của lane là `native-engine` (chính mgc giữ
    # install — shared store, CAS, lockfile), không dimension nào đi nhờ
    # toolchain gốc. Đây là verdict DUY NHẤT đủ chứng minh claim "native
    # multi-language package manager". Lane có install delegated vẫn là
    # orchestrator nhưng KHÔNG BAO GIỜ native-pm — hai claim gate khác
    # nhau về marketing lẫn release.
    def _satisfied(status):
        return status in PASS_STATUSES

    for r in results:
        dims = r["dimensions"]
        required = r.pop("required_dimensions")
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
            r["native_pm_verdict"] = "not-native-pm"
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
        # native-dimension requirement remain additional gates for
        # native-pm-supported.
        # (Verdict native-PM (cổng P0-D): `native-pm-supported` CHỈ khi
        # dependency_owner của lane là `mgc-native` — toolchain ủy quyền
        # không bao giờ đủ cho claim native dù dimension điểm thế nào.
        # Lane delegated trần ở verdict MỚI `compatibility-passed` (chứng
        # minh tương thích toolchain, KHÔNG phải sở hữu của mgc);
        # `scaffold-only`/`unsupported` đọc `unsupported`. Taxonomy chủ
        # sở hữu install (P0-D vòng-9) và yêu cầu dimension native vẫn là
        # các cổng bổ sung cho native-pm-supported.)
        dep_owner = r.get("dependency_owner", "")
        r["native_pm_verdict"] = (
            "native-pm-supported"
            if required
            and dep_owner == "mgc-native"
            and all(dims[d] == STATUS_NATIVE for d in required)
            and not native_pm_delegated
            and install_owner == "native-engine"
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
        "matrix_kind": "lifecycle",
        # The six-value status vocabulary (Gate 11-C schema v2) so every
        # consumer interprets statuses identically — only *-pass statuses
        # satisfy, unverified/unsupported/failed satisfy nothing.
        # (Bộ 6 trạng thái (Gate 11-C schema v2) để mọi consumer đọc
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
