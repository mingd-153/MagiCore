#!/usr/bin/env python3
"""
Regression tests for strict analyzer validation
Tests that invalid samples are properly rejected
"""

import json
import math
import tempfile
from pathlib import Path
from analyze_results_strict import validate_result, ValidationError

def test_case(name: str, data: dict, should_fail: bool = True):
    """Run single test case"""
    try:
        validate_result(data, "test.json")
        if should_fail:
            print(f"❌ FAIL: {name} - Expected rejection but passed")
            return False
        else:
            print(f"✅ PASS: {name}")
            return True
    except ValidationError as e:
        if should_fail:
            print(f"✅ PASS: {name} - Rejected as expected: {e}")
            return True
        else:
            print(f"❌ FAIL: {name} - Unexpected rejection: {e}")
            return False
    except Exception as e:
        print(f"❌ FAIL: {name} - Unexpected error: {e}")
        return False

def valid_sample():
    """Return a valid baseline sample"""
    return {
        'pm': 'test',
        'run': 1,
        'timestamp': '2026-09-04T12:00:00Z',
        'package_count': 20,
        'cold_install': {
            'duration_seconds': 2.5,
            'disk_mb': 450,
            'exit_code': 0
        },
        'warm_install': {
            'duration_seconds': 1.8,
            'exit_code': 0
        }
    }

def main():
    print("Running strict analyzer regression tests...\n")

    tests_passed = 0
    tests_total = 0

    # Test 1: Valid sample should pass
    tests_total += 1
    if test_case("Valid sample", valid_sample(), should_fail=False):
        tests_passed += 1

    # Test 2: Missing required field
    tests_total += 1
    sample = valid_sample()
    del sample['pm']
    if test_case("Missing pm field", sample, should_fail=True):
        tests_passed += 1

    # Test 3: Negative duration
    tests_total += 1
    sample = valid_sample()
    sample['cold_install']['duration_seconds'] = -1.5
    if test_case("Negative cold duration", sample, should_fail=True):
        tests_passed += 1

    # Test 4: NaN value
    tests_total += 1
    sample = valid_sample()
    sample['warm_install']['duration_seconds'] = float('nan')
    if test_case("NaN warm duration", sample, should_fail=True):
        tests_passed += 1

    # Test 5: Infinity value
    tests_total += 1
    sample = valid_sample()
    sample['cold_install']['disk_mb'] = float('inf')
    if test_case("Infinite disk_mb", sample, should_fail=True):
        tests_passed += 1

    # Test 6: Failed exit code (cold)
    tests_total += 1
    sample = valid_sample()
    sample['cold_install']['exit_code'] = 1
    if test_case("Failed cold install (exit 1)", sample, should_fail=True):
        tests_passed += 1

    # Test 7: Failed exit code (warm)
    tests_total += 1
    sample = valid_sample()
    sample['warm_install']['exit_code'] = 127
    if test_case("Failed warm install (exit 127)", sample, should_fail=True):
        tests_passed += 1

    # Test 8: Invalid package_count
    tests_total += 1
    sample = valid_sample()
    sample['package_count'] = 0
    if test_case("Zero package_count", sample, should_fail=True):
        tests_passed += 1

    # Test 9: Invalid package_count type
    tests_total += 1
    sample = valid_sample()
    sample['package_count'] = "20"
    if test_case("String package_count", sample, should_fail=True):
        tests_passed += 1

    # Test 10: Zero duration (should fail - must be positive)
    tests_total += 1
    sample = valid_sample()
    sample['cold_install']['duration_seconds'] = 0
    if test_case("Zero duration", sample, should_fail=True):
        tests_passed += 1

    # Test 11: Negative disk_mb
    tests_total += 1
    sample = valid_sample()
    sample['cold_install']['disk_mb'] = -100
    if test_case("Negative disk_mb", sample, should_fail=True):
        tests_passed += 1

    # Test 12: Missing exit_code
    tests_total += 1
    sample = valid_sample()
    del sample['cold_install']['exit_code']
    if test_case("Missing cold exit_code", sample, should_fail=True):
        tests_passed += 1

    # Test 13: Non-integer exit_code
    tests_total += 1
    sample = valid_sample()
    sample['warm_install']['exit_code'] = "0"
    if test_case("String exit_code", sample, should_fail=True):
        tests_passed += 1

    # Test 14: Empty PM string
    tests_total += 1
    sample = valid_sample()
    sample['pm'] = ""
    if test_case("Empty pm string", sample, should_fail=True):
        tests_passed += 1

    # Test 15: Empty timestamp
    tests_total += 1
    sample = valid_sample()
    sample['timestamp'] = ""
    if test_case("Empty timestamp", sample, should_fail=True):
        tests_passed += 1

    print(f"\n{'='*60}")
    print(f"Results: {tests_passed}/{tests_total} tests passed")

    if tests_passed == tests_total:
        print("✅ All regression tests passed!")
        return 0
    else:
        print(f"❌ {tests_total - tests_passed} test(s) failed")
        return 1

if __name__ == '__main__':
    exit(main())
