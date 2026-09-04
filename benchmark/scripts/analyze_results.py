#!/usr/bin/env python3
"""
P1.1 FIX: Statistical analysis for benchmark results
Calculates: median, p95, stddev, min, max for 20-30 runs
"""

import json
import sys
from pathlib import Path
from typing import List, Dict
import statistics

def load_results(results_dir: Path, pm_name: str) -> List[Dict]:
    """Load all JSON results for a PM"""
    results = []
    pattern = f"{pm_name}_run*.json"

    for json_file in sorted(results_dir.glob(pattern)):
        try:
            with open(json_file) as f:
                data = json.load(f)
                results.append(data)
        except Exception as e:
            print(f"⚠️  Failed to load {json_file}: {e}", file=sys.stderr)

    return results

def extract_metrics(results: List[Dict]) -> Dict[str, List[float]]:
    """Extract cold/warm times and disk usage"""
    metrics = {
        'cold_time': [],
        'warm_time': [],
        'disk_mb': []
    }

    for result in results:
        if 'cold_time_sec' in result:
            metrics['cold_time'].append(result['cold_time_sec'])
        if 'warm_time_sec' in result:
            metrics['warm_time'].append(result['warm_time_sec'])
        if 'disk_mb' in result:
            metrics['disk_mb'].append(result['disk_mb'])

    return metrics

def calculate_stats(values: List[float]) -> Dict:
    """Calculate statistical measures"""
    if not values:
        return {
            'n': 0,
            'min': None,
            'max': None,
            'mean': None,
            'median': None,
            'p95': None,
            'stddev': None,
            'cv': None
        }

    sorted_values = sorted(values)
    n = len(values)

    # Percentile calculation
    p95_index = int(n * 0.95)
    p95 = sorted_values[min(p95_index, n-1)]

    mean = statistics.mean(values)
    median = statistics.median(values)
    stddev = statistics.stdev(values) if n > 1 else 0
    cv = (stddev / mean * 100) if mean > 0 else 0

    return {
        'n': n,
        'min': min(values),
        'max': max(values),
        'mean': round(mean, 2),
        'median': round(median, 2),
        'p95': round(p95, 2),
        'stddev': round(stddev, 2),
        'cv': round(cv, 1)
    }

def print_summary(pm_name: str, metrics: Dict[str, List[float]]):
    """Print statistical summary"""
    print(f"\n=== {pm_name.upper()} Statistics ===\n")

    for metric_name, values in metrics.items():
        if not values:
            continue

        stats = calculate_stats(values)
        label = metric_name.replace('_', ' ').title()

        print(f"{label}:")
        print(f"  Runs: {stats['n']}")
        print(f"  Min: {stats['min']}")
        print(f"  Median: {stats['median']}")
        print(f"  Mean: {stats['mean']}")
        print(f"  P95: {stats['p95']}")
        print(f"  Max: {stats['max']}")
        print(f"  StdDev: {stats['stddev']}")
        print(f"  CV: {stats['cv']}%")
        print()

def save_analysis(pm_name: str, metrics: Dict[str, List[float]], output_file: Path):
    """Save analysis to JSON"""
    analysis = {
        'pm': pm_name,
        'timestamp': __import__('datetime').datetime.now().isoformat(),
        'metrics': {}
    }

    for metric_name, values in metrics.items():
        analysis['metrics'][metric_name] = calculate_stats(values)

    with open(output_file, 'w') as f:
        json.dump(analysis, f, indent=2)

    print(f"✅ Analysis saved: {output_file}")

def main():
    if len(sys.argv) < 2:
        print("Usage: ./analyze_results.py <pm_name> [results_dir]")
        print("Example: ./analyze_results.py mgc")
        sys.exit(1)

    pm_name = sys.argv[1]
    results_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent.parent / 'results' / 'p1_suite'

    if not results_dir.exists():
        print(f"❌ Results directory not found: {results_dir}")
        sys.exit(1)

    print(f"Loading results for {pm_name} from {results_dir}...")
    results = load_results(results_dir, pm_name)

    if not results:
        print(f"❌ No results found for {pm_name}")
        sys.exit(1)

    print(f"✅ Loaded {len(results)} results")

    metrics = extract_metrics(results)
    print_summary(pm_name, metrics)

    output_file = results_dir / f"{pm_name}_analysis.json"
    save_analysis(pm_name, metrics, output_file)

if __name__ == '__main__':
    main()
