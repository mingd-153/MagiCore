#!/usr/bin/env python3
"""
Analyze benchmark results and generate report
"""

import json
import glob
import statistics
from pathlib import Path

def load_results(pm_name):
    """Load all results for a package manager"""
    pattern = f"../results/{pm_name}_run*.json"
    files = sorted(glob.glob(pattern))
    
    results = []
    for f in files:
        if '.old' in f:
            continue
        with open(f) as file:
            data = json.load(file)
            results.append(data)
    
    return results

def analyze_pm(pm_name):
    """Analyze results for one PM"""
    results = load_results(pm_name)
    
    if not results:
        return None
    
    cold_times = [float(r['cold_install']['duration_seconds']) for r in results]
    warm_times = [float(r['warm_install']['duration_seconds']) for r in results]
    disk_sizes = [int(r['cold_install']['disk_mb']) for r in results]
    
    return {
        'pm': pm_name,
        'runs': len(results),
        'cold': {
            'mean': statistics.mean(cold_times),
            'median': statistics.median(cold_times),
            'stdev': statistics.stdev(cold_times) if len(cold_times) > 1 else 0,
            'min': min(cold_times),
            'max': max(cold_times),
        },
        'warm': {
            'mean': statistics.mean(warm_times),
            'median': statistics.median(warm_times),
            'stdev': statistics.stdev(warm_times) if len(warm_times) > 1 else 0,
            'min': min(warm_times),
            'max': max(warm_times),
        },
        'disk': {
            'mean': statistics.mean(disk_sizes),
            'median': statistics.median(disk_sizes),
            'stdev': statistics.stdev(disk_sizes) if len(disk_sizes) > 1 else 0,
        },
        'package_count': results[0].get('package_count', 'N/A'),
        'machine': results[0].get('machine', {}),
    }

def format_time(seconds):
    """Format seconds to human readable"""
    if seconds < 1:
        return f"{seconds*1000:.0f}ms"
    elif seconds < 60:
        return f"{seconds:.2f}s"
    else:
        mins = int(seconds // 60)
        secs = seconds % 60
        return f"{mins}m {secs:.1f}s"

def main():
    pms = ['npm', 'pnpm', 'bun', 'mgc']
    
    print("=" * 80)
    print("MAGICORE BENCHMARK ANALYSIS")
    print("=" * 80)
    print()
    
    # Analyze each PM
    stats = {}
    for pm in pms:
        result = analyze_pm(pm)
        if result:
            stats[pm] = result
    
    if not stats:
        print("ERROR: No results found!")
        return
    
    # Print machine info
    machine = list(stats.values())[0]['machine']
    print("Machine Specs:")
    print(f"  CPU: {machine.get('cpu', 'N/A')}")
    print(f"  Cores: {machine.get('cores', 'N/A')}")
    print(f"  Memory: {machine.get('memory_gb', 'N/A')} GB")
    print(f"  OS: {machine.get('os', 'N/A')}")
    print(f"  Node: {machine.get('node_version', 'N/A')}")
    print()
    
    # Print cold install comparison
    print("COLD INSTALL (Fresh, no cache):")
    print("-" * 80)
    print(f"{'PM':<8} {'Mean':<12} {'Median':<12} {'StdDev':<12} {'Min':<12} {'Max':<12}")
    print("-" * 80)
    
    for pm in pms:
        if pm in stats:
            s = stats[pm]['cold']
            print(f"{pm:<8} {format_time(s['mean']):<12} {format_time(s['median']):<12} "
                  f"{format_time(s['stdev']):<12} {format_time(s['min']):<12} {format_time(s['max']):<12}")
    
    print()
    
    # Print warm install comparison
    print("WARM INSTALL (With cache):")
    print("-" * 80)
    print(f"{'PM':<8} {'Mean':<12} {'Median':<12} {'StdDev':<12} {'Min':<12} {'Max':<12}")
    print("-" * 80)
    
    for pm in pms:
        if pm in stats:
            s = stats[pm]['warm']
            print(f"{pm:<8} {format_time(s['mean']):<12} {format_time(s['median']):<12} "
                  f"{format_time(s['stdev']):<12} {format_time(s['min']):<12} {format_time(s['max']):<12}")
    
    print()
    
    # Print disk usage
    print("DISK USAGE (node_modules size):")
    print("-" * 80)
    print(f"{'PM':<8} {'Mean':<12} {'Median':<12} {'StdDev':<12}")
    print("-" * 80)
    
    for pm in pms:
        if pm in stats:
            s = stats[pm]['disk']
            print(f"{pm:<8} {s['mean']:.1f} MB{'':<4} {s['median']:.1f} MB{'':<4} {s['stdev']:.1f} MB")
    
    print()
    
    # Speedup comparisons
    print("SPEEDUP COMPARISONS (vs npm baseline):")
    print("-" * 80)
    
    npm_cold = stats['npm']['cold']['median']
    npm_warm = stats['npm']['warm']['median']
    npm_disk = stats['npm']['disk']['median']
    
    for pm in ['pnpm', 'bun', 'mgc']:
        if pm in stats:
            cold_speedup = npm_cold / stats[pm]['cold']['median']
            warm_speedup = npm_warm / stats[pm]['warm']['median']
            disk_reduction = 100 * (1 - stats[pm]['disk']['median'] / npm_disk)
            
            print(f"{pm.upper()}:")
            print(f"  Cold install: {cold_speedup:.1f}x faster")
            print(f"  Warm install: {warm_speedup:.1f}x faster")
            print(f"  Disk usage: {disk_reduction:+.1f}% (vs npm)")
            print()
    
    # Ranking
    print("RANKINGS:")
    print("-" * 80)
    
    # Cold install ranking
    cold_sorted = sorted(stats.items(), key=lambda x: x[1]['cold']['median'])
    print("Cold Install (fastest to slowest):")
    for i, (pm, s) in enumerate(cold_sorted, 1):
        print(f"  {i}. {pm:<6} {format_time(s['cold']['median'])}")
    print()
    
    # Warm install ranking
    warm_sorted = sorted(stats.items(), key=lambda x: x[1]['warm']['median'])
    print("Warm Install (fastest to slowest):")
    for i, (pm, s) in enumerate(warm_sorted, 1):
        print(f"  {i}. {pm:<6} {format_time(s['warm']['median'])}")
    print()
    
    # Disk usage ranking
    disk_sorted = sorted(stats.items(), key=lambda x: x[1]['disk']['median'])
    print("Disk Usage (smallest to largest):")
    for i, (pm, s) in enumerate(disk_sorted, 1):
        print(f"  {i}. {pm:<6} {s['disk']['median']:.1f} MB")
    print()
    
    print("=" * 80)
    
    # Save JSON summary
    summary = {
        'machine': machine,
        'package_count': list(stats.values())[0]['package_count'],
        'stats': stats,
        'rankings': {
            'cold': [pm for pm, _ in cold_sorted],
            'warm': [pm for pm, _ in warm_sorted],
            'disk': [pm for pm, _ in disk_sorted],
        }
    }
    
    with open('../results/summary.json', 'w') as f:
        json.dump(summary, f, indent=2)
    
    print("Summary saved to: benchmark/results/summary.json")

if __name__ == '__main__':
    main()
