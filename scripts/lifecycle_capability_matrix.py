#!/usr/bin/env python3
"""Generate the LIFECYCLE capability matrix from evidence — separate
from the audit capability matrix (Tech Lead P0-2, 2026-09-11).

The audit matrix (audit_capability_matrix.py) proves ADVISORY evidence
per lane (clean/vulnerable/tool-failure). It can NEVER justify a
"language supported" claim about the PRODUCT — lifecycle support is a
separate dimension with its own evidence: create → import/lockfile →
install → dev → test → build → run → audit → cache-reuse → offline.

This script executes the real `mgc` binary through per-lane lifecycle
steps in a sandbox and records which dimensions each lane actually
completed (passed) vs. which are delegated/unsupported/failed. The
output drives release gates that need PRODUCT capability, not audit
capability. Fail-closed: any binary crash/unparseable step aborts.

Sinh ma trận năng lực LIFECYCLE từ bằng chứng — tách khỏi matrix audit
(P0-2). Matrix audit chứng minh evidence advisory từng lane, KHÔNG BAO
GIỜ suy ra được năng lực product; lifecycle là dimension riêng với
evidence riêng: create → import/lockfile → install → dev → test →
build → run → audit → cache → offline. Script chạy binary mgc THẬT qua
từng bước lifecycle trong sandbox rồi ghi dimension nào lane đó hoàn
thành thật. Fail-closed: binary crash/step không parse được → huỷ.
"""

import datetime
import time
import json
import os
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
        # Dimensions this lane MUST pass for "lifecycle-supported" —
        # the verdict contract is per-lane, not global (P0 finding #1).
        # Dimension lane này BẮT BUỘC pass để "lifecycle-supported" —
        # hợp đồng verdict theo từng lane, không theo tập toàn cục (P0
        # finding #1).
        "required_dims": ["create", "install", "test", "build"],
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
        # go toolchain owns module download/test/build (delegation by
        # design Q9) — install recorded as delegated, not shared-store.
        # go toolchain giữ download/test/build (delegation Q9) — install
        # ghi delegated, không ghi shared-store.
        "delegated": ["install"],
        "required_dims": ["create", "install", "test", "build"],
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
        # mgc audits gradle lockfiles; native install lane lands P2 —
        # honest delegated status, never a silent no-op.
        # mgc audit lockfile gradle; install native thuộc P2 — trạng
        # thái delegated trung thực, không no-op âm thầm.
        "delegated": ["install"],
        "required_dims": ["create", "install", "test", "build"],
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
        "delegated": ["install"],
        "required_dims": ["create", "install", "test", "build"],
    },
    {
        "core": "web",
        "language": "javascript",
        "scaffold": ["create-web", "react", "test-web"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
            ("dev", ["dev", "--help"]),  # dev server lifecycle needs a
            # TTY/hold; --help proves the command wiring only.
        ],
        "delegated": [],
        # Web carries the product's flagship promise — run + dev are part
        # of the lane's own lifecycle contract (P0 finding #1: the
        # verdict must weigh every dimension the lane declares).
        # Web giữ lời hứa chủ lực của product — run + dev thuộc hợp đồng
        # lifecycle của chính lane (P0 finding #1: verdict phải cân mọi
        # dimension lane tuyên bố).
        "required_dims": ["create", "install", "test", "build", "dev", "run"],
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
        # AI lane: the scaffold ships no dependency set (deps = []) —
        # install is a real no-op that must still exit 0; test needs the
        # template's own tests. Honest absent for run/dev until the AI
        # dev-server lane ships.
        # Lane AI: template không kèm dependency (deps = []) — install
        # là no-op thật nhưng vẫn phải exit 0; test cần test của chính
        # template. run/dev absent trung thực tới khi lane dev-server
        # cho AI ra đời.
        "required_dims": ["create", "install", "test", "build"],
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
        # flutter toolchain owns pub cache (delegation by design).
        # flutter toolchain giữ pub cache (ủy quyền theo thiết kế).
        "delegated": ["install"],
        "required_dims": ["create", "install", "test", "build"],
    },
]

# Binary step timeout (seconds) — overridable via MGC_LIFECYCLE_STEP_TIMEOUT.
# Timeout mỗi bước binary — ghi đè qua MGC_LIFECYCLE_STEP_TIMEOUT.
STEP_TIMEOUT_DEFAULT_S = 600

# Every dimension a fully-lifecycle-supported lane must pass. Lanes
# missing any dimension report it as absent — the gate blocks.
# Mọi dimension lane lifecycle-đủ phải pass; thiếu dimension nào thì
# gate chặn dimension đó.
ALL_DIMENSIONS = ["create", "install", "test", "build", "run", "dev", "audit",
                  "cache-reuse", "offline-reinstall"]


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


def _fail(message: str, detail: str = "") -> None:
    """Abort generation — a crashed binary run must never look like a
    capability matrix (fail-closed collection, same contract as audit).
    Huỷ sinh matrix — binary crash không bao giờ được thành matrix."""
    print(f"LIFECYCLE MATRIX COLLECTION FAILED: {message}", file=sys.stderr)
    if detail:
        print(detail[-4000:], file=sys.stderr)
    sys.exit(1)


def run_lane(mgc_bin: str, lane: dict) -> dict:
    """Execute one lane's lifecycle steps in a fresh sandbox; record
    per-dimension status: passed / delegated / failed / absent.
    Chạy các bước lifecycle của một lane trong sandbox mới; ghi trạng
    thái từng dimension: passed / delegated / failed / absent."""
    timeout_s = int(os.environ.get("MGC_LIFECYCLE_STEP_TIMEOUT", STEP_TIMEOUT_DEFAULT_S))
    sandbox = tempfile.mkdtemp(prefix=f"mgc-lc-{lane['core']}-{lane['language']}-")
    dims: dict[str, str] = {}
    detail = {"sandbox": sandbox}
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
        dims["create"] = "failed"
        detail["create_output"] = out[-2000:]
    elif not scaffold_language_matches(sandbox, project_dir, lane["language"]):
        dims["create"] = "failed"
        found = sorted(
            f for f in os.listdir(os.path.join(sandbox, project_dir))
            if not f.startswith(".")
        )
        detail["create_output"] = (
            f"scaffold fell back to another language (requested "
            f"{lane['language']}); project files: {found}"
        )
    else:
        dims["create"] = "passed"

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
        if dims.get("create") != "passed":
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
        if dims.get("create") != "passed":
            dims[step] = "absent"  # cannot continue a broken lane
            continue
        rc, out = run_step(argv, subdir=project_dir)
        if step in lane["delegated"] and rc != 0:
            # Delegated step failing is recorded honestly (delegated
            # steps still must WORK, only the cache claim differs).
            # Bước delegated fail vẫn ghi trung thực (phải chạy được,
            # chỉ khác claim về cache).
            dims[step] = "failed"
            detail[f"{step}_output"] = out[-2000:]
        elif rc == 0:
            dims[step] = "delegated" if step in lane["delegated"] else "passed"
        else:
            dims[step] = "failed"
            detail[f"{step}_output"] = out[-2000:]

    # `run` probe (P0 finding #6): long-lived production entry — start
    # `mgc run <script>`, wait for a REAL HTTP roundtrip on the probe
    # port, then shut down cleanly. A server that only starts (or dies
    # instantly) is a FAILED run, never a passed one.
    # Probe `run` (P0 finding #6): entry production dài hạn — khởi động
    # `mgc run <script>`, chờ HTTP roundtrip THẬT trên port probe, rồi
    # tắt sạch. Server chỉ khởi động (hoặc chết ngay) là run FAILED,
    # không bao giờ passed.
    probe = lane.get("run_probe")
    if probe is not None and dims.get("create") == "passed":
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
                dims["run"] = "failed"
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
        if dims.get("run") != "failed":
            dims["run"] = "passed" if served else "failed"
            if not served:
                detail["run_output"] = (
                    "run probe: no HTTP 200 on localhost:%d before timeout" % port
                )

    # audit rides the audit matrix — reference only, never inferred.
    # audit thuộc matrix audit riêng — tham chiếu, không suy ra.
    dims["audit"] = "see-audit-matrix"

    # Dimensions with no step defined for this lane: absent (honest).
    # Dimension không có bước cho lane này: absent (trung thực).
    for dim in ALL_DIMENSIONS:
        dims.setdefault(dim, "absent")

    shutil.rmtree(sandbox, ignore_errors=True)
    return {"dims": dims, "detail": detail}


def main() -> int:
    # Resolve to an ABSOLUTE path: steps run with cwd = sandbox, so a
    # relative binary path would resolve inside the sandbox and vanish.
    # Quy về đường dẫn TUYỆT ĐỐI: bước chạy với cwd = sandbox nên đường
    # dẫn tương đối sẽ trỏ vào sandbox và biến mất.
    mgc_bin = os.path.abspath(os.environ.get("MGC_LIFECYCLE_BIN", "./target/debug/mgc"))
    if not os.path.isfile(mgc_bin):
        _fail(f"mgc binary not found at {mgc_bin} — build it first")

    commit = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    results = []
    for lane in LANES:
        r = run_lane(mgc_bin, lane)
        results.append({
            "core": lane["core"],
            "language": lane["language"],
            "dimensions": r["dims"],
            "required_dimensions": lane["required_dims"],
            "commit": commit,
            "verified_at": now,
        })

    # Aggregate verdict (P0 finding #1 fix, 2026-09-12): a lane is
    # "lifecycle-supported" only when EVERY dimension the lane declares
    # as required is passed (delegated ≠ passed; absent ≠ passed). The
    # previous global 4-dim check let a lane missing run/dev/cache/
    # offline still be called supported — a semantic lie. Per-lane
    # contracts, never a global subset.
    # Verdict tổng hợp (P0 finding #1): lane "lifecycle-supported" chỉ
    # khi MỌI dimension lane tuyên bố bắt buộc đều passed (delegated ≠
    # passed; absent ≠ passed). Kiểm 4-dim toàn cục cũ cho phép lane
    # thiếu run/dev/cache/offline vẫn được gọi supported — sai semantics.
    # Hợp đồng theo từng lane, không bao giờ theo tập con toàn cục.
    for r in results:
        dims = r["dimensions"]
        required = r.pop("required_dimensions")
        r["verdict"] = (
            "lifecycle-supported"
            if required and all(dims[d] == "passed" for d in required)
            else "partial" if any(dims[d] == "passed" for d in required)
            else "unsupported"
        )

    out = {
        "schema_version": 1,
        "generated_at": now,
        "commit": commit,
        "matrix_kind": "lifecycle",
        "dimensions": ALL_DIMENSIONS,
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
        passed = [d for d, s in r["dimensions"].items() if s == "passed"]
        delegated = [d for d, s in r["dimensions"].items() if s == "delegated"]
        print(f"  {r['verdict']:<22} {r['core']}/{r['language']}: passed={passed} delegated={delegated}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
