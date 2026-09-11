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
    },
    {
        "core": "lib",
        "language": "python",
        "scaffold": ["create-lib", "python", "test-lib"],
        "steps": [
            ("install", ["install"]),
            ("test", ["test"]),
            ("build", ["build"]),
        ],
        "delegated": [],
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
            "commit": commit,
            "verified_at": now,
        })

    # Aggregate verdict: a lane is "lifecycle-supported" only when every
    # product dimension is passed (delegated ≠ passed; absent ≠ passed).
    # Verdict tổng hợp: lane "lifecycle-supported" chỉ khi mọi dimension
    # product đều passed (delegated ≠ passed; absent ≠ passed).
    for r in results:
        dims = r["dimensions"]
        product_dims = [d for d in ("create", "install", "test", "build") if d in dims]
        r["verdict"] = (
            "lifecycle-supported"
            if all(dims[d] == "passed" for d in product_dims) and product_dims
            else "partial" if any(dims[d] == "passed" for d in product_dims)
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
