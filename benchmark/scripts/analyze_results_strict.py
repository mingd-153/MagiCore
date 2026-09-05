#!/usr/bin/env python3
"""
STRICT benchmark analyzer with comprehensive validation
Rejects malformed/incomplete samples, reports what was rejected

Validates:
- Schema completeness (required fields present)
- Numeric validity (finite, positive durations/sizes)
- Exit codes (success = 0 only)
- PM match (data['pm'] must match requested PM)
- Run uniqueness (no duplicate run IDs)
- ISO-8601 timestamp parsing
- Workload consistency (same package_count)
- Optional metadata (pm_version, mgc_commit, session_id, manifest_hash, lockfile_hash)

Enhanced for RC-3: Strict validation ensures benchmark claims are defensible.
"""

import json
import sys
import math
from pathlib import Path
from typing import List, Dict, Optional, Tuple
import statistics
from datetime import datetime

# Required schema fields
REQUIRED_FIELDS = ['pm', 'run', 'timestamp', 'package_count', 'cold_install', 'warm_install']
REQUIRED_COLD = ['duration_seconds', 'disk_mb', 'exit_code']
REQUIRED_WARM = ['duration_seconds', 'exit_code']

class ValidationError(Exception):
    pass

def validate_result(data: Dict, filename: str) -> None:
    """Strict validation - raise on any issue"""
    # Check top-level required fields
    missing = [f for f in REQUIRED_FIELDS if f not in data]
    if missing:
        raise ValidationError(f"Missing fields: {missing}")

    # Check cold_install
    if not isinstance(data['cold_install'], dict):
        raise ValidationError("cold_install must be object")

    missing_cold = [f for f in REQUIRED_COLD if f not in data['cold_install']]
    if missing_cold:
        raise ValidationError(f"cold_install missing: {missing_cold}")

    # Check warm_install
    if not isinstance(data['warm_install'], dict):
        raise ValidationError("warm_install must be object")

    missing_warm = [f for f in REQUIRED_WARM if f not in data['warm_install']]
    if missing_warm:
        raise ValidationError(f"warm_install missing: {missing_warm}")

    # Validate numeric values: must be finite and positive
    cold_duration = float(data['cold_install']['duration_seconds'])
    cold_disk = float(data['cold_install']['disk_mb'])
    warm_duration = float(data['warm_install']['duration_seconds'])

    # Check finite
    if not math.isfinite(cold_duration):
        raise ValidationError(f"cold_install.duration_seconds not finite: {cold_duration}")
    if not math.isfinite(cold_disk):
        raise ValidationError(f"cold_install.disk_mb not finite: {cold_disk}")
    if not math.isfinite(warm_duration):
        raise ValidationError(f"warm_install.duration_seconds not finite: {warm_duration}")

    # Check positive
    if cold_duration <= 0:
        raise ValidationError(f"cold_install.duration_seconds must be positive: {cold_duration}")
    if cold_disk < 0:
        raise ValidationError(f"cold_install.disk_mb must be non-negative: {cold_disk}")
    if warm_duration <= 0:
        raise ValidationError(f"warm_install.duration_seconds must be positive: {warm_duration}")

    # Validate exit codes (0 = success)
    cold_exit = data['cold_install']['exit_code']
    warm_exit = data['warm_install']['exit_code']

    if not isinstance(cold_exit, int):
        raise ValidationError(f"cold_install.exit_code must be int: {type(cold_exit)}")
    if not isinstance(warm_exit, int):
        raise ValidationError(f"warm_install.exit_code must be int: {type(warm_exit)}")

    if cold_exit != 0:
        raise ValidationError(f"cold_install failed: exit_code={cold_exit}")
    if warm_exit != 0:
        raise ValidationError(f"warm_install failed: exit_code={warm_exit}")

    # Check package_count consistency (all runs must have same workload)
    if not isinstance(data['package_count'], int) or data['package_count'] <= 0:
        raise ValidationError(f"Invalid package_count: {data['package_count']}")

    # Validate PM and timestamp
    if not isinstance(data['pm'], str) or not data['pm'].strip():
        raise ValidationError(f"Invalid pm field: {data.get('pm')}")

    if not isinstance(data['timestamp'], str) or not data['timestamp'].strip():
        raise ValidationError(f"Invalid timestamp: {data.get('timestamp')}")

    # Validate run number (must be positive integer)
    if not isinstance(data['run'], int) or data['run'] <= 0:
        raise ValidationError(f"Invalid run number: {data.get('run')}")

    # Parse ISO-8601 timestamp
    try:
        datetime.fromisoformat(data['timestamp'].replace('Z', '+00:00'))
    except (ValueError, AttributeError) as e:
        raise ValidationError(f"Invalid ISO-8601 timestamp: {data['timestamp']} ({e})")

    # Enhanced metadata validation (optional but validated if present)
    if 'pm_version' in data:
        if not isinstance(data['pm_version'], str) or not data['pm_version'].strip():
            raise ValidationError(f"Invalid pm_version: {data.get('pm_version')}")

    if 'mgc_commit' in data:
        if not isinstance(data['mgc_commit'], str) or len(data['mgc_commit']) < 7:
            raise ValidationError(f"Invalid mgc_commit (need 7+ chars): {data.get('mgc_commit')}")

    if 'session_id' in data:
        if not isinstance(data['session_id'], str) or not data['session_id'].strip():
            raise ValidationError(f"Invalid session_id: {data.get('session_id')}")

    if 'manifest_hash' in data:
        if not isinstance(data['manifest_hash'], str) or len(data['manifest_hash']) < 8:
            raise ValidationError(f"Invalid manifest_hash (need 8+ chars): {data.get('manifest_hash')}")

    if 'lockfile_hash' in data:
        if not isinstance(data['lockfile_hash'], str) or len(data['lockfile_hash']) < 8:
            raise ValidationError(f"Invalid lockfile_hash (need 8+ chars): {data.get('lockfile_hash')}")

def load_results_strict(results_dir: Path, pm_name: str, expected_packages: Optional[int] = None) -> Tuple[List[Dict], List[str]]:
    """
    Load and validate results
    Returns: (valid_results, rejection_reasons)
    """
    valid = []
    rejections = []
    pattern = f"{pm_name}_run*.json"
    seen_runs = set()  # Track run IDs for uniqueness

    # Filter out runlarge files
    files = [f for f in results_dir.glob(pattern) if 'runlarge' not in f.name]

    if not files:
        return [], [f"No files matching pattern: {pattern} (excluding runlarge)"]

    # Infer expected_packages from first valid file if not provided
    if expected_packages is None:
        for json_file in sorted(files):
            try:
                with open(json_file) as f:
                    data = json.load(f)
                    expected_packages = data.get('package_count')
                    if expected_packages:
                        break
            except:
                continue

    for json_file in sorted(files):
        try:
            with open(json_file) as f:
                data = json.load(f)

            # Validate schema
            validate_result(data, json_file.name)

            # Validate PM match (data['pm'] must match requested pm_name)
            if data['pm'].lower() != pm_name.lower():
                rejections.append(f"{json_file.name}: PM mismatch (file has '{data['pm']}', expected '{pm_name}')")
                continue

            # Check run uniqueness
            run_id = data['run']
            if run_id in seen_runs:
                rejections.append(f"{json_file.name}: Duplicate run ID {run_id}")
                continue
            seen_runs.add(run_id)

            # Check workload consistency
            if expected_packages and data['package_count'] != expected_packages:
                rejections.append(f"{json_file.name}: Wrong workload ({data['package_count']} != {expected_packages} packages)")
                continue

            # Check for extreme outliers (likely errors) - >10x median cold time
            cold_time = float(data['cold_install']['duration_seconds'])
            if cold_time > 3600:  # >1 hour is likely timeout/hang
                rejections.append(f"{json_file.name}: Extreme cold time ({cold_time}s) - likely timeout")
                continue

            valid.append(data)

        except json.JSONDecodeError as e:
            rejections.append(f"{json_file.name}: Invalid JSON - {e}")
        except ValidationError as e:
            rejections.append(f"{json_file.name}: {e}")
        except Exception as e:
            rejections.append(f"{json_file.name}: Unexpected error - {e}")

    return valid, rejections

def extract_metrics_strict(results: List[Dict]) -> Dict[str, List[float]]:
    """Extract metrics - assumes already validated"""
    metrics = {
        'cold_time': [],
        'warm_time': [],
        'disk_mb': []
    }

    for result in results:
        metrics['cold_time'].append(float(result['cold_install']['duration_seconds']))
        metrics['warm_time'].append(float(result['warm_install']['duration_seconds']))
        metrics['disk_mb'].append(float(result['cold_install']['disk_mb']))

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
        'min': round(min(values), 2),
        'max': round(max(values), 2),
        'mean': round(mean, 2),
        'median': round(median, 2),
        'p95': round(p95, 2),
        'stddev': round(stddev, 2),
        'cv': round(cv, 1)
    }

def print_summary(pm_name: str, metrics: Dict[str, List[float]], rejections: List[str]):
    """Print statistical summary with rejections"""
    print(f"\n=== {pm_name.upper()} Strict Analysis ===\n")

    if rejections:
        print(f"⚠️  REJECTED {len(rejections)} samples:")
        for reason in rejections[:10]:  # Show first 10
            print(f"  - {reason}")
        if len(rejections) > 10:
            print(f"  ... and {len(rejections) - 10} more")
        print()

    total_valid = len(metrics['cold_time'])
    print(f"✅ VALID samples: {total_valid}\n")

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

def save_analysis(pm_name: str, metrics: Dict[str, List[float]], rejections: List[str], output_file: Path):
    """Save analysis to JSON"""
    analysis = {
        'pm': pm_name,
        'timestamp': datetime.now().isoformat(),
        'validation': {
            'valid_samples': len(metrics['cold_time']),
            'rejected_samples': len(rejections),
            'rejection_reasons': rejections
        },
        'metrics': {}
    }

    for metric_name, values in metrics.items():
        analysis['metrics'][metric_name] = calculate_stats(values)

    with open(output_file, 'w') as f:
        json.dump(analysis, f, indent=2)

    print(f"✅ Analysis saved: {output_file}")

def main():
    if len(sys.argv) < 2:
        print("Usage: ./analyze_results_strict.py <pm_name> [results_dir] [expected_packages]")
        print("Example: ./analyze_results_strict.py mgc results/ 20")
        sys.exit(1)

    pm_name = sys.argv[1]
    results_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent.parent / 'results'
    expected_packages = int(sys.argv[3]) if len(sys.argv) > 3 else None

    if not results_dir.exists():
        print(f"❌ Results directory not found: {results_dir}")
        sys.exit(1)

    print(f"Loading results for {pm_name} from {results_dir}...")
    if expected_packages:
        print(f"Expected workload: {expected_packages} packages")

    results, rejections = load_results_strict(results_dir, pm_name, expected_packages)

    if not results:
        print(f"\n❌ No valid results found for {pm_name}")
        if rejections:
            print("\nRejection reasons:")
            for r in rejections:
                print(f"  - {r}")
        sys.exit(1)

    metrics = extract_metrics_strict(results)
    print_summary(pm_name, metrics, rejections)

    output_file = results_dir / f"{pm_name}_analysis_strict.json"
    save_analysis(pm_name, metrics, rejections, output_file)

if __name__ == '__main__':
    main()
