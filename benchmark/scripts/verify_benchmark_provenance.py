#!/usr/bin/env python3
"""Verify benchmark provenance — Xác minh nguồn gốc dữ liệu benchmark."""

from __future__ import annotations

import json
import math
import re
import statistics
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PHASED_RESULTS = ROOT / "benchmark" / "results" / "phased"


@dataclass(frozen=True)
class Measurements:
    cold: tuple[float, ...]
    warm: tuple[float, ...]
    disk_mb: tuple[int, ...]
    machine: tuple[str, int, str]

    @property
    def cold_mean(self) -> float:
        return statistics.mean(self.cold)

    @property
    def warm_mean(self) -> float:
        return statistics.mean(self.warm)

    @property
    def cold_stdev(self) -> float:
        return statistics.stdev(self.cold)

    @property
    def warm_stdev(self) -> float:
        return statistics.stdev(self.warm)

    @property
    def cold_cv(self) -> int:
        return round(self.cold_stdev / self.cold_mean * 100)

    @property
    def warm_cv(self) -> int:
        return round(self.warm_stdev / self.warm_mean * 100)


def load_suite(package_manager: str) -> Measurements:
    """Load one phased suite — Đọc một bộ kết quả phased duy nhất."""
    paths = sorted(PHASED_RESULTS.glob(f"{package_manager}_run*.json"))
    if len(paths) != 5:
        raise ValueError(f"expected 5 {package_manager} runs, found {len(paths)}")

    cold: list[float] = []
    warm: list[float] = []
    disk_mb: list[int] = []
    machines: set[tuple[str, int, str]] = set()

    for expected_run, path in enumerate(paths, start=1):
        data = json.loads(path.read_text(encoding="utf-8"))
        if data.get("pm") != package_manager or data.get("run") != expected_run:
            raise ValueError(f"unexpected identity in {path}")
        if not str(data.get("timestamp", "")).startswith("20260828_"):
            raise ValueError(f"mixed benchmark date in {path}")

        machine = data.get("machine", {})
        machines.add((machine.get("cpu"), machine.get("cores"), machine.get("os")))
        cold.append(float(data["cold"]["seconds"]))
        warm.append(float(data["warm"]["seconds"]))
        disk_mb.append(int(data["cold"]["disk_mb"]))

    if len(machines) != 1:
        raise ValueError(f"mixed machines in {package_manager} suite: {machines}")
    return Measurements(tuple(cold), tuple(warm), tuple(disk_mb), next(iter(machines)))


def require_text(path: Path, snippets: tuple[str, ...]) -> list[str]:
    """Require canonical snippets — Bắt buộc tài liệu chứa số liệu chuẩn."""
    text = path.read_text(encoding="utf-8")
    normalized_text = re.sub(r"\s+", " ", text)
    return [
        f"{path.relative_to(ROOT)} missing: {snippet}"
        for snippet in snippets
        if re.sub(r"\s+", " ", snippet) not in normalized_text
    ]


def main() -> int:
    """Validate raw data and public docs — Kiểm tra raw data và tài liệu public."""
    mgc = load_suite("mgc")
    pnpm = load_suite("pnpm")
    if mgc.machine != pnpm.machine:
        raise ValueError(f"mixed machines across suites: mgc={mgc.machine}, pnpm={pnpm.machine}")
    warm_ratio = mgc.warm_mean / pnpm.warm_mean
    disk_overhead = round((mgc.disk_mb[0] - pnpm.disk_mb[0]) / pnpm.disk_mb[0] * 100)

    mgc_rows = tuple(
        f"| {run}   | {cold:<8.2f} | {warm:<8.2f} | {disk:<9} |"
        for run, (cold, warm, disk) in enumerate(zip(mgc.cold, mgc.warm, mgc.disk_mb), start=1)
    )
    pnpm_rows = tuple(
        f"| {run}   | {cold:<8.2f} | {warm:<8.2f} |"
        for run, (cold, warm) in enumerate(zip(pnpm.cold, pnpm.warm), start=1)
    )

    expected = {
        ROOT / "benchmark" / "results" / "BENCHMARK_SUMMARY_V1.0_FINAL.md": (
            f"**pnpm**: {pnpm.cold_mean:.2f}s ± {pnpm.cold_stdev:.2f}s",
            f"**pnpm**: {pnpm.warm_mean:.2f}s ± {pnpm.warm_stdev:.2f}s",
            *mgc_rows,
            *pnpm_rows,
            f"| **Mean** | **{pnpm.cold_mean:.2f}** | **{pnpm.warm_mean:.2f}** |",
            f"| **CV** | **{pnpm.cold_cv}%** | **{pnpm.warm_cv}%** |",
            f"Measured: {mgc.cold_mean:.2f}s vs {pnpm.cold_mean:.2f}s pnpm average (5 runs each)",
            f"**pnpm**: {pnpm.disk_mb[0]}MB (hardlink store)",
        ),
        ROOT / "README.md": (
            f"| **Cold Install** | {mgc.cold_mean:.1f}s | {pnpm.cold_mean:.0f}s |",
            f"| **Warm Install** | {mgc.warm_mean:.1f}s | {pnpm.warm_mean:.1f}s | pnpm {warm_ratio:.1f}x faster",
            f"pnpm {pnpm.cold_cv}%",
            f"| **Disk Usage** | {mgc.disk_mb[0]}MB | {pnpm.disk_mb[0]}MB | +{disk_overhead}% CAS overhead |",
        ),
        ROOT / "CHANGELOG.md": (
            f"**Cold install**: {mgc.cold_mean:.2f}s average",
            f"**Warm install**: {mgc.warm_mean:.2f}s (pnpm {warm_ratio:.1f}x faster",
        ),
        ROOT / "benchmark" / "BENCHMARK_METHODOLOGY.md": (
            f"{pnpm.cold_mean:.2f}s / {mgc.cold_mean:.2f}s",
            f"{mgc.cold_mean:.1f}s vs {pnpm.cold_mean:.0f}s pnpm",
            f"High CV ({pnpm.cold_cv}%)",
        ),
    }

    errors: list[str] = []
    for path, snippets in expected.items():
        errors.extend(require_text(path, snippets))

    # Reject stale absolute marketing claims — Chặn claim marketing tuyệt đối đã lỗi thời.
    public_text = "\n".join(path.read_text(encoding="utf-8") for path in expected)
    if re.search(r"\b(?:39|45(?:\.7)?)x\s+(?:faster|slower|speedup)\b", public_text, re.IGNORECASE):
        errors.append("stale absolute performance multiplier remains in public documentation")

    if errors:
        print("Benchmark provenance verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    if not math.isclose(pnpm.cold[4], 151.050628, rel_tol=0.0, abs_tol=1e-9):
        print("Benchmark provenance verification failed: unexpected phased pnpm run 5", file=sys.stderr)
        return 1

    print("Benchmark provenance verified from phased 2026-08-28 results.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
