#!/usr/bin/env python3
"""Aggregate all per-cell run JSONs under benchmark/results/v2 into the
final SUMMARY_PROVISIONAL.json + comparison table (steady-state focus).

PROVISIONAL label (Tech Lead audit 2026-09-13): this aggregator still does
NOT fail-closed on missing runs/failed runs/mixed provenance — treat every
output as a raw local experiment, not a validated benchmark.

Reads every <pm>_<workload>_<mode>/run*.json produced by bench_v2.py and
computes per-cell stats (all runs + steady = excluding first_cold), plus a
head-to-head comparison table against mgc.
"""
# Gộp toàn bộ run JSON theo cell thành SUMMARY_PROVISIONAL.json + bảng so sánh
# (steady-state làm trọng tâm) — chạy sau bench_v2.py. Nhãn PROVISIONAL
# (audit 2026-09-13): aggregator chưa fail-closed với run thiếu/fail/provenance
# trộn — mọi output chỉ là thí nghiệm local thô, chưa phải benchmark validated.

import json
import math
import statistics
import sys
from datetime import datetime, timezone
from pathlib import Path

RESULTS = Path(__file__).resolve().parents[1] / "results" / "v2"


def stats(values):
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


def main():
    cells = {}
    raw_all = []
    for cell_dir in sorted(RESULTS.iterdir()):
        if not cell_dir.is_dir():
            continue
        runs = []
        for f in sorted(cell_dir.glob("run*.json")):
            runs.append(json.loads(f.read_text()))
        if not runs:
            continue
        label = cell_dir.name
        ok_times = [r["duration_seconds"] for r in runs if r["ok"]]
        steady = [
            r["duration_seconds"]
            for r in runs
            if r["ok"] and not r.get("first_cold")
        ]
        disk = [r["disk_mb"] for r in runs if r["ok"] and r.get("disk_mb")]
        cells[label] = {
            "ok_runs": len(ok_times),
            "total_runs": len(runs),
            "duration": stats(ok_times),
            "duration_steady": stats(steady) if steady else {},
            "disk_mb": stats(disk) if disk else {},
        }
        raw_all.extend(runs)

    first_run = raw_all[0] if raw_all else {}
    hardware = first_run.get("hardware", {})

    # Head-to-head vs mgc (steady medians).
    # So sánh trực tiếp với mgc (median steady).
    compare = {}
    for wl in ["small", "medium", "large"]:
        for mode in ["cold", "warm"]:
            mgc = cells.get(f"mgc_{wl}_{mode}", {}).get("duration_steady", {})
            row = {}
            for pm in ["pnpm", "bun", "npm"]:
                other = cells.get(f"{pm}_{wl}_{mode}", {}).get(
                    "duration_steady", {}
                )
                if mgc and other and other.get("median"):
                    row[pm] = {
                        "mgc_vs": round(mgc["median"] / other["median"], 2),
                        "faster": "mgc" if mgc["median"] < other["median"] else pm,
                    }
            compare[f"{wl}_{mode}"] = row

    summary = {
        "generated": datetime.now(timezone.utc).isoformat(),
        "hardware": hardware,
        "cells": cells,
        "mgc_vs_others_steady_median_ratio": compare,
        "note": "ratio >1 means mgc SLOWER (mgc_median / other_median). "
        "See BENCHMARK_STATUS.md for interpretation rules.",
    }
    out = RESULTS / "SUMMARY_PROVISIONAL.json"
    out.write_text(json.dumps(summary, indent=2))
    print(f"cells aggregated: {len(cells)}")
    print(f"written: {out}")

    # Compact table for terminal.
    # Bảng gọn cho terminal.
    print(f"\n{'cell':<24}{'n':>4}{'median':>9}{'p95':>9}{'cv%':>7}")
    for label, c in sorted(cells.items()):
        st = c["duration_steady"] or c["duration"]
        if st:
            print(
                f"{label:<24}{st['n']:>4}{st['median']:>9.2f}"
                f"{st['p95']:>9.2f}{st['cv_percent']:>7}"
            )
    print("\nmgc vs others (steady median, lower mgc_median/other_median = mgc faster):")
    for k, row in compare.items():
        line = f"  {k}: "
        for pm, v in row.items():
            line += f"{pm}={v['mgc_vs']}x ({v['faster']} faster)  "
        print(line)


if __name__ == "__main__":
    sys.exit(main())
