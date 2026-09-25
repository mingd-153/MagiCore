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
    """Validate raw data and withdrawn-claim state — Kiểm tra dữ liệu và trạng thái rút claim."""
    mgc = load_suite("mgc")
    pnpm = load_suite("pnpm")
    if mgc.machine != pnpm.machine:
        raise ValueError(f"mixed machines across suites: mgc={mgc.machine}, pnpm={pnpm.machine}")

    invalidated_docs = {
        ROOT / "benchmark" / "BENCHMARK_STATUS.md": ("INVALIDATED", "WITHDRAWN"),
        ROOT / "benchmark" / "results" / "MGC_VS_PNPM_VALIDATED.md": (
            "INVALIDATED",
            "Do not cite or use for performance claims",
        ),
    }
    errors: list[str] = []
    for path, snippets in invalidated_docs.items():
        errors.extend(require_text(path, snippets))

    # Public release docs must not reactivate invalidated comparisons.
    # Tài liệu phát hành public không được kích hoạt lại so sánh đã invalidated.
    public_paths = (ROOT / "README.md", ROOT / "CHANGELOG.md")
    public_text = "\n".join(path.read_text(encoding="utf-8") for path in public_paths)
    if re.search(r"\b\d+(?:\.\d+)?x\s+(?:faster|slower|speedup)\b", public_text, re.IGNORECASE):
        errors.append("active performance multiplier remains in public release documentation")
    if "2.63s average" in public_text or "2.01s" in public_text:
        errors.append("withdrawn V1.0 benchmark measurements remain in public release documentation")

    if errors:
        print("Benchmark provenance verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    if not math.isclose(pnpm.cold[4], 151.050628, rel_tol=0.0, abs_tol=1e-9):
        print("Benchmark provenance verification failed: unexpected phased pnpm run 5", file=sys.stderr)
        return 1

    print("Legacy benchmark provenance parsed; public performance claims remain withdrawn.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
