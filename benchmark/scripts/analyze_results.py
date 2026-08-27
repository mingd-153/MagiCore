#!/usr/bin/env python3
"""
MagiCore Benchmark Results Analyzer
Calculates mean, median, stddev from JSON results
Generates comparison table for BENCHMARK.md
"""

import json
import sys
from pathlib import Path
from statistics import mean, median, stdev
from collections import defaultdict

def load_results(results_dir: Path):
    """Load all JSON result files"""
    results = defaultdict(list)
    
    for json_file in results_dir.glob("*.json"):
        try:
            with open(json_file) as f:
                data = json.load(f)
                pm = data["pm"]
                results[pm].append(data)
        except Exception as e:
            print(f"⚠️  Error loading {json_file}: {e}", file=sys.stderr)
    
    return results

def calculate_stats(values: list) -> dict:
    """Calculate mean, median, stddev"""
    if not values:
        return {"mean": 0, "median": 0, "stddev": 0}
    
    return {
        "mean": mean(values),
        "median": median(values),
        "stddev": stdev(values) if len(values) > 1 else 0,
        "min": min(values),
        "max": max(values),
        "count": len(values)
    }

def analyze_pm(runs: list) -> dict:
    """Analyze all runs for a single PM"""
    cold_durations = [r["cold_install"]["duration_seconds"] for r in runs]
    warm_durations = [r["warm_install"]["duration_seconds"] for r in runs]
    disk_usage = [r["cold_install"]["disk_mb"] for r in runs]
    
    return {
        "cold_install": calculate_stats(cold_durations),
        "warm_install": calculate_stats(warm_durations),
        "disk_usage": calculate_stats(disk_usage),
        "runs": len(runs)
    }

def format_duration(seconds: float) -> str:
    """Format seconds as human-readable duration"""
    if seconds < 1:
        return f"{seconds * 1000:.0f}ms"
    elif seconds < 60:
        return f"{seconds:.1f}s"
    else:
        minutes = int(seconds // 60)
        secs = seconds % 60
        return f"{minutes}m {secs:.0f}s"

def format_disk(mb: float) -> str:
    """Format MB as human-readable size"""
    if mb < 1024:
        return f"{mb:.0f}MB"
    else:
        return f"{mb / 1024:.1f}GB"

def print_comparison_table(analysis: dict):
    """Print markdown comparison table"""
    print("\n## Benchmark Results — Cold Install (5 Runs)\n")
    print("| PM | Mean | Median | StdDev | Min | Max | Disk Usage |")
    print("|---|---|---|---|---|---|---|")
    
    # Sort by mean duration
    sorted_pms = sorted(analysis.items(), key=lambda x: x[1]["cold_install"]["mean"])
    
    for pm, stats in sorted_pms:
        cold = stats["cold_install"]
        disk = stats["disk_usage"]
        
        print(f"| **{pm}** "
              f"| {format_duration(cold['mean'])} "
              f"| {format_duration(cold['median'])} "
              f"| ±{format_duration(cold['stddev'])} "
              f"| {format_duration(cold['min'])} "
              f"| {format_duration(cold['max'])} "
              f"| {format_disk(disk['mean'])} |")
    
    print("\n## Benchmark Results — Warm Install (Cached)\n")
    print("| PM | Mean | Median | StdDev | Min | Max |")
    print("|---|---|---|---|---|---|")
    
    for pm, stats in sorted_pms:
        warm = stats["warm_install"]
        
        print(f"| **{pm}** "
              f"| {format_duration(warm['mean'])} "
              f"| {format_duration(warm['median'])} "
              f"| ±{format_duration(warm['stddev'])} "
              f"| {format_duration(warm['min'])} "
              f"| {format_duration(warm['max'])} |")

def print_speed_comparison(analysis: dict):
    """Print relative speed comparison"""
    print("\n## Relative Speed (vs Fastest)\n")
    
    sorted_pms = sorted(analysis.items(), key=lambda x: x[1]["cold_install"]["mean"])
    fastest_duration = sorted_pms[0][1]["cold_install"]["mean"]
    
    print("| PM | Cold Install | Speedup | Disk Efficiency |")
    print("|---|---|---|---|")
    
    for pm, stats in sorted_pms:
        cold = stats["cold_install"]["mean"]
        disk = stats["disk_usage"]["mean"]
        ratio = cold / fastest_duration
        
        if ratio == 1.0:
            speedup = "**1.0x** (baseline)"
        else:
            speedup = f"{ratio:.2f}x slower"
        
        print(f"| **{pm}** "
              f"| {format_duration(cold)} "
              f"| {speedup} "
              f"| {format_disk(disk)} |")

def print_summary(analysis: dict):
    """Print executive summary"""
    sorted_pms = sorted(analysis.items(), key=lambda x: x[1]["cold_install"]["mean"])
    
    print("\n## Executive Summary\n")
    print(f"**Fastest PM:** {sorted_pms[0][0]} ({format_duration(sorted_pms[0][1]['cold_install']['mean'])} cold install)")
    print(f"**Slowest PM:** {sorted_pms[-1][0]} ({format_duration(sorted_pms[-1][1]['cold_install']['mean'])} cold install)")
    print(f"**Speed Range:** {sorted_pms[-1][1]['cold_install']['mean'] / sorted_pms[0][1]['cold_install']['mean']:.1f}x difference")
    print()
    
    # Disk efficiency
    sorted_disk = sorted(analysis.items(), key=lambda x: x[1]["disk_usage"]["mean"])
    print(f"**Most Efficient Disk:** {sorted_disk[0][0]} ({format_disk(sorted_disk[0][1]['disk_usage']['mean'])})")
    print(f"**Largest Disk:** {sorted_disk[-1][0]} ({format_disk(sorted_disk[-1][1]['disk_usage']['mean'])})")
    print(f"**Disk Range:** {sorted_disk[-1][1]['disk_usage']['mean'] / sorted_disk[0][1]['disk_usage']['mean']:.1f}x difference")

def main():
    if len(sys.argv) < 2:
        print("Usage: analyze_results.py <results_dir>", file=sys.stderr)
        sys.exit(1)
    
    results_dir = Path(sys.argv[1])
    
    if not results_dir.exists():
        print(f"Error: {results_dir} does not exist", file=sys.stderr)
        sys.exit(1)
    
    print("🔍 Loading benchmark results...")
    results = load_results(results_dir)
    
    if not results:
        print("❌ No valid JSON results found", file=sys.stderr)
        sys.exit(1)
    
    print(f"✓ Found results for PMs: {', '.join(results.keys())}")
    print(f"✓ Total runs: {sum(len(runs) for runs in results.values())}")
    
    print("\n🧮 Analyzing data...")
    analysis = {pm: analyze_pm(runs) for pm, runs in results.items()}
    
    print_summary(analysis)
    print_comparison_table(analysis)
    print_speed_comparison(analysis)
    
    print("\n✅ Analysis complete!\n")
    print("📋 Copy the tables above into docs/BENCHMARK.md")
    print("🚧 Remove the 'PRELIMINARY' status after verification")

if __name__ == "__main__":
    main()
