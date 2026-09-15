#!/usr/bin/env python3
"""Benchmark v2 — PROVISIONAL PM comparison (relabelled per Tech Lead audit
2026-09-13: raw local experiment, NOT validated for marketing claims).

Design (adversarial-review requirements):
- 3 workloads: small (5 deps), medium (10 deps), large (30 deps)
- cold (cache wiped) + warm (cache kept, node_modules removed)
- >= 30 runs per (pm, workload, mode) — median/p95/CV reported
- raw JSON per run + summary table; hardware + commit recorded
- strict methodology: no scripts, same fixture set, fresh workspace per run,
  caches cleaned per PM, no concurrent load, sleep between runs

Output: benchmark/results/v2/<label>/*.json + SUMMARY_PROVISIONAL.json + .md
"""
# Benchmark v2 — so sánh PM có bằng chứng: 3 workload × cold/warm × >=30 run,
# raw JSON + median/p95/CV + hardware + commit. Không chạy song song.
# Nhãn PROVISIONAL (audit 2026-09-13): thí nghiệm local thô — chưa đủ chuẩn
# claim marketing (provenance/apples-to-apples còn thiếu).

import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BENCH = REPO / "benchmark"
RESULTS = BENCH / "results" / "v2"
ENV = BENCH / "env"
WORK_ROOT = Path("/tmp/mgc_bench_v2")
MGC_BIN = REPO / "target" / "release" / "mgc"
GIT_SHA = os.environ.get("BENCH_GIT_SHA", "unknown")

RUNS_PER_CELL = 30
SLEEP_BETWEEN = 2

WORKLOADS = {
    "small": ENV / "package-small.json",
    "medium": ENV / "package-unified.json",
    "large": ENV / "package-large.json",
}

PMS = {
    "mgc": {
        "install": ["--core", "web", "install"],
        "cache_clean": ["rm_store"],
        "bin": [str(MGC_BIN)],
    },
    "pnpm": {
        "install": ["install", "--ignore-scripts"],
        "cache_clean": ["pnpm_store_prune"],
        "bin": ["pnpm"],
    },
    "bun": {
        "install": ["install", "--ignore-scripts"],
        "cache_clean": ["bun_cache_rm"],
        "bin": ["bun"],
    },
    "npm": {
        "install": ["install", "--ignore-scripts", "--no-audit", "--no-fund"],
        "cache_clean": ["npm_cache_clean"],
        "bin": ["npm"],
    },
}


def sh(cmd: list, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def clean_pm_cache(pm: str) -> None:
    """Reset PM cache so cold means cold (download + extract from network)."""
    # Xóa cache PM để cold là cold thật (tải + giải nén từ mạng).
    if pm == "mgc":
        shutil.rmtree(Path.home() / ".magicore" / "store", ignore_errors=True)
        shutil.rmtree(Path.home() / ".magicore" / "cache", ignore_errors=True)
    elif pm == "pnpm":
        sh(["pnpm", "store", "prune"])
    elif pm == "bun":
        shutil.rmtree(Path.home() / ".bun" / "install" / "cache", ignore_errors=True)
    elif pm == "npm":
        sh(["npm", "cache", "clean", "--force"])


def disk_mb(path: Path) -> int:
    r = sh(["du", "-sm", str(path)])
    try:
        return int(r.stdout.split()[0])
    except (IndexError, ValueError):
        return 0


def run_install(pm: str, workdir: Path) -> tuple[float, bool, str]:
    """One timed install; returns (seconds, ok, log tail)."""
    # Một lần install có đo giờ; trả (giây, thành công, log cuối).
    spec = PMS[pm]
    cmd = list(spec["bin"]) + spec["install"]
    t0 = time.perf_counter()
    r = subprocess.run(cmd, cwd=workdir, capture_output=True, text=True)
    t1 = time.perf_counter()
    ok = r.returncode == 0 and (workdir / "node_modules").exists()
    tail = ((r.stdout or "") + (r.stderr or ""))[-400:]
    return t1 - t0, ok, tail


def bench_cell(pm: str, workload: str, mode: str, run: int) -> dict:
    """One benchmark cell: fresh workspace, fixture copied, timed install.

    Cold mode wipes the PM cache per run, but the FIRST cold run also pays
    OS-level warmup (DNS, TLS session, registry metadata) that later runs
    inherit for free — that run is flagged `first_cold` so analysis can
    report steady-state cold separately without hiding data.
    """
    # Một cell: workspace mới, copy fixture, install có đo giờ. Cold run đầu
    # còn chịu phí warmup của OS (DNS/TLS/metadata) — đánh dấu first_cold
    # để phân tích tách cold ổn định, không giấu dữ liệu.
    work = WORK_ROOT / f"{pm}_{workload}_{mode}_{run}_{os.getpid()}"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    shutil.copy(WORKLOADS[workload], work / "package.json")

    if mode == "cold":
        clean_pm_cache(pm)

    # Prime cache only for warm runs (cold download happened before).
    # Warm: giữ cache, chỉ xóa node_modules. Cần cache tồn tại từ run trước.
    if mode == "warm" and run == 1:
        # First warm run needs a primed cache — install once, then measure.
        # Run warm đầu cần cache đã có — install một lần trước khi đo.
        run_install(pm, work)
        shutil.rmtree(work / "node_modules")

    secs, ok, log = run_install(pm, work)
    du = disk_mb(work / "node_modules") if ok else 0
    shutil.rmtree(work, ignore_errors=True)
    time.sleep(SLEEP_BETWEEN)
    return {
        "pm": pm,
        "workload": workload,
        "mode": mode,
        "run": run,
        "first_cold": mode == "cold" and run == 1,
        "duration_seconds": round(secs, 3),
        "disk_mb": du,
        "ok": ok,
        "log_tail": log,
    }


def stats(values: list) -> dict:
    if not values:
        return {}
    s = sorted(values)
    p95_idx = max(0, math.ceil(0.95 * len(s)) - 1)
    mean = statistics.mean(s)
    return {
        "n": len(s),
        "median": round(statistics.median(s), 3),
        "mean": round(mean, 3),
        "p95": round(s[p95_idx], 3),
        "min": round(s[0], 3),
        "max": round(s[-1], 3),
        "stdev": round(statistics.stdev(s), 3) if len(s) > 1 else 0.0,
        "cv_percent": round(100 * statistics.stdev(s) / mean, 1)
        if len(s) > 1 and mean > 0
        else 0.0,
    }


def machine_spec() -> dict:
    cpu = sh(["sysctl", "-n", "machdep.cpu.brand_string"]).stdout.strip()
    cores = sh(["sysctl", "-n", "hw.ncpu"]).stdout.strip()
    mem = int(sh(["sysctl", "-n", "hw.memsize"]).stdout.strip() or 0) // 1024**3
    return {
        "cpu": cpu,
        "cores": cores,
        "memory_gb": mem,
        "os": f"{sh(['uname', '-s']).stdout.strip()} {sh(['uname', '-r']).stdout.strip()}",
        "git_sha": GIT_SHA,
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }


def main() -> int:
    only_pms = sys.argv[1].split(",") if len(sys.argv) > 1 else list(PMS)
    only_wl = sys.argv[2].split(",") if len(sys.argv) > 2 else list(WORKLOADS)
    runs = int(sys.argv[3]) if len(sys.argv) > 3 else RUNS_PER_CELL

    RESULTS.mkdir(parents=True, exist_ok=True)
    WORK_ROOT.mkdir(parents=True, exist_ok=True)

    spec = machine_spec()
    print(f"hardware: {spec['cpu']} | {spec['cores']} cores | {spec['os']}")
    print(f"runs per cell: {runs} | pms: {only_pms} | workloads: {only_wl}")

    raw = []
    summary = {"hardware": spec, "methodology": {
        "runs_per_cell": runs,
        "modes": ["cold", "warm"],
        "scripts": "disabled (--ignore-scripts / mgc runs install only)",
        "fixture": "benchmark/env/package-{small,unified,large}.json",
        "fresh_workspace": "per run (/tmp/mgc_bench_v2)",
        "cache_clean": "cold wipes ~/.magicore/store|cache, pnpm store prune, "
                       "~/.bun/install/cache, npm cache clean --force",
    }, "cells": {}}

    for pm in only_pms:
        if pm not in PMS:
            print(f"skip unknown pm {pm}")
            continue
        for wl in only_wl:
            for mode in ["cold", "warm"]:
                label = f"{pm}_{wl}_{mode}"
                cell_dir = RESULTS / label
                cell_dir.mkdir(parents=True, exist_ok=True)
                times, oks = [], 0
                raw_cell = []
                for i in range(1, runs + 1):
                    res = bench_cell(pm, wl, mode, i)
                    res["hardware"] = spec
                    (cell_dir / f"run{i:02d}.json").write_text(
                        json.dumps(res, indent=2)
                    )
                    raw.append(res)
                    raw_cell.append(res)
                    if res["ok"]:
                        times.append(res["duration_seconds"])
                        oks += 1
                    print(
                        f"[{label}] run {i:02d}/{runs}: "
                        f"{res['duration_seconds']:.2f}s ok={res['ok']}"
                    )
                # Steady cold = runs 2..N (run 1 pays OS warmup: DNS/TLS/
                # registry metadata) — reported separately, raw kept intact.
                # Cold ổn định = run 2..N (run 1 trả phí warmup của OS) —
                # báo cáo tách riêng, raw giữ nguyên.
                steady = [
                    r["duration_seconds"]
                    for r in raw_cell
                    if r["ok"] and not r.get("first_cold")
                ]
                summary["cells"][label] = {
                    "ok_runs": oks,
                    "total_runs": runs,
                    "duration": stats(times),
                    "duration_steady": stats(steady) if steady else {},
                }
                print(f"== {label}: {summary['cells'][label]['duration']}")
                print(f"== {label} (steady): {summary['cells'][label]['duration_steady']}")

    (RESULTS / "RAW_ALL_RUNS.json").write_text(json.dumps(raw, indent=2))
    (RESULTS / "SUMMARY_PROVISIONAL.json").write_text(
        json.dumps(summary, indent=2)
    )
    print(f"\nwritten: {RESULTS / 'SUMMARY_PROVISIONAL.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
