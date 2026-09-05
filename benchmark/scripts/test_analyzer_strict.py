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


# NEW TESTS: Enhanced validations from RC-3

def test_new_validations():
    """Test new validation rules added in RC-3"""
    print("\n=== Testing RC-3 Enhanced Validations ===\n")

    tests_passed = 0
    tests_total = 0

    # Test 16: Invalid ISO-8601 timestamp
    tests_total += 1
    sample = valid_sample()
    sample['timestamp'] = "not-iso-format"
    if test_case("Invalid ISO timestamp", sample, should_fail=True):
        tests_passed += 1

    # Test 17: Negative run number
    tests_total += 1
    sample = valid_sample()
    sample['run'] = -1
    if test_case("Negative run number", sample, should_fail=True):
        tests_passed += 1

    # Test 18: Zero run number
    tests_total += 1
    sample = valid_sample()
    sample['run'] = 0
    if test_case("Zero run number", sample, should_fail=True):
        tests_passed += 1

    # Test 19: Invalid pm_version (if present)
    tests_total += 1
    sample = valid_sample()
    sample['pm_version'] = ""
    if test_case("Empty pm_version", sample, should_fail=True):
        tests_passed += 1

    # Test 20: Invalid mgc_commit (too short)
    tests_total += 1
    sample = valid_sample()
    sample['mgc_commit'] = "abc"
    if test_case("Short mgc_commit (<7 chars)", sample, should_fail=True):
        tests_passed += 1

    # Test 21: Invalid session_id
    tests_total += 1
    sample = valid_sample()
    sample['session_id'] = ""
    if test_case("Empty session_id", sample, should_fail=True):
        tests_passed += 1

    # Test 22: Invalid manifest_hash (too short)
    tests_total += 1
    sample = valid_sample()
    sample['manifest_hash'] = "abc123"  # Not 64 chars
    if test_case("Short manifest_hash (<64 chars)", sample, should_fail=True):
        tests_passed += 1

    # Test 23: Invalid lockfile_hash (too short)
    tests_total += 1
    sample = valid_sample()
    sample['lockfile_hash'] = "xyz"  # Not 64 chars
    if test_case("Short lockfile_hash (<64 chars)", sample, should_fail=True):
        tests_passed += 1

    # Test 24: Valid sample with all provenance fields
    tests_total += 1
    sample = valid_sample()
    sample['pm_version'] = "1.1.0"
    sample['mgc_commit'] = "cedd3c28645"
    sample['session_id'] = "test-session-20260905"
    sample['manifest_hash'] = "a" * 64  # Valid 64-char hex
    sample['lockfile_hash'] = "b" * 64  # Valid 64-char hex
    if test_case("Valid with provenance", sample, should_fail=False):
        tests_passed += 1

    # Test 25: Publish mode requires provenance
    tests_total += 1
    from analyze_results_strict import validate_result
    sample = valid_sample()
    try:
        validate_result(sample, "test.json", publish_mode=True)
        print(f"❌ FAIL: Publish mode without provenance - Should have been rejected")
    except ValidationError as e:
        if "provenance" in str(e).lower():
            print(f"✅ PASS: Publish mode without provenance - Rejected as expected")
            tests_passed += 1
        else:
            print(f"❌ FAIL: Publish mode - Wrong error: {e}")

    # Test 26: Publish mode accepts complete provenance
    tests_total += 1
    sample = valid_sample()
    sample['pm_version'] = "1.1.0"
    sample['mgc_commit'] = "abc123def"
    sample['session_id'] = "sess-12345678"
    sample['manifest_hash'] = "c" * 64  # Valid 64-char hex
    sample['lockfile_hash'] = "d" * 64  # Valid 64-char hex
    try:
        validate_result(sample, "test.json", publish_mode=True)
        print(f"✅ PASS: Publish mode with provenance - Accepted")
        tests_passed += 1
    except ValidationError as e:
        print(f"❌ FAIL: Publish mode with provenance - Unexpected rejection: {e}")

    # Test 27: Non-integer run type
    tests_total += 1
    sample = valid_sample()
    sample['run'] = "1"
    if test_case("String run number", sample, should_fail=True):
        tests_passed += 1

    # Test 28: Float run number
    tests_total += 1
    sample = valid_sample()
    sample['run'] = 1.5
    if test_case("Float run number", sample, should_fail=True):
        tests_passed += 1

    print(f"\n{'='*60}")
    print(f"RC-3 Tests: {tests_passed}/{tests_total} passed")

    return tests_passed, tests_total

def test_adversarial_provenance():
    """Test strict provenance validation against adversarial inputs"""
    print("\n=== Testing Adversarial Provenance ===\n")

    tests_passed = 0
    tests_total = 0

    # Test 29: Non-hex commit
    tests_total += 1
    sample = valid_sample()
    sample['mgc_commit'] = "not-hex!@#"
    if test_case("Non-hex commit", sample, should_fail=True):
        tests_passed += 1

    # Test 30: manifest_hash non-hex
    tests_total += 1
    sample = valid_sample()
    sample['manifest_hash'] = "g" * 64
    if test_case("manifest_hash non-hex", sample, should_fail=True):
        tests_passed += 1

    # Test 31: lockfile_hash wrong length
    tests_total += 1
    sample = valid_sample()
    sample['lockfile_hash'] = "abc" * 20  # 60 chars, not 64
    if test_case("lockfile_hash wrong length", sample, should_fail=True):
        tests_passed += 1

    # Test 32: session_id too short
    tests_total += 1
    sample = valid_sample()
    sample['session_id'] = "short"  # Only 5 chars
    if test_case("session_id too short (<8)", sample, should_fail=True):
        tests_passed += 1

    # Test 33: session_id with spaces
    tests_total += 1
    sample = valid_sample()
    sample['session_id'] = "invalid spaces"
    if test_case("session_id with spaces", sample, should_fail=True):
        tests_passed += 1

    # Test 34: pm_version non-semver
    tests_total += 1
    sample = valid_sample()
    sample['pm_version'] = "not-semver"
    if test_case("pm_version non-semver", sample, should_fail=True):
        tests_passed += 1

    # Test 35: Valid strict provenance
    tests_total += 1
    sample = valid_sample()
    sample['pm_version'] = "1.1.0"
    sample['mgc_commit'] = "cedd3c28645f8a2d18464ebe02b4773ef7fae875"
    sample['session_id'] = "test-sess-20260905"
    sample['manifest_hash'] = "a1b2c3d4" * 8  # 64 hex chars
    sample['lockfile_hash'] = "e5f6a7b8" * 8  # 64 hex chars
    if test_case("Valid strict provenance", sample, should_fail=False):
        tests_passed += 1

    print(f"\n{'='*60}")
    print(f"Adversarial Tests: {tests_passed}/{tests_total} passed")

    return tests_passed, tests_total

if __name__ == '__main__':
    # Run original tests
    original_result = main()

    # Run new RC-3 tests
    new_passed, new_total = test_new_validations()

    # Run adversarial tests
    adv_passed, adv_total = test_adversarial_provenance()

    # Combined summary
    print(f"\n{'='*60}")
    print(f"COMBINED RESULTS:")
    print(f"  Original tests: 15/15 passed" if original_result == 0 else f"  Original tests: FAILED")
    print(f"  RC-3 tests: {new_passed}/{new_total} passed")
    print(f"  Adversarial tests: {adv_passed}/{adv_total} passed")
    print(f"  TOTAL: {15 + new_passed + adv_passed}/{15 + new_total + adv_total} tests")

    if original_result == 0 and new_passed == new_total and adv_passed == adv_total:
        print("\n✅ ALL TESTS PASSED")
        exit(0)
    else:
        print(f"\n❌ SOME TESTS FAILED")
        exit(1)
