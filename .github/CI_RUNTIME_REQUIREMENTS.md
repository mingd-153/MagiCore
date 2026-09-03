# CI Runtime Requirements for MagiCore Tests

## Current Blocker: Tests Fail Due to Missing Runtimes

Tests now FAIL LOUDLY with `panic!("UNVERIFIED")` when runtimes missing.
This is CORRECT behavior - tests must not pass when unverified.

For tests to become VERIFIED, CI must provision these runtimes:

## Required Runtimes by Test Suite

### 1. Optimizer Tests (`cli/tests/optimizer_lifecycle_e2e.rs`)

**AI (Python) Optimizer:**
- Runtime: `python3` + `pytest`
- Installation: `pip install pytest` or `apt-get install python3-pytest`
- Test: `test_ai_python_mgc_test_with_optimizer`
- Evidence needed: OPTIMIZER_STATUS marker in pytest child process

**App (Flutter) Optimizer:**
- Runtime: Flutter SDK
- Installation: https://docs.flutter.dev/get-started/install
- Test: `test_app_flutter_mgc_build_with_optimizer`
- Evidence needed: OPTIMIZER_STATUS marker in Flutter child process

### 2. Lifecycle Tests (`cli/tests/full_lifecycle_e2e.rs`)

**Web Lifecycle:**
- Runtime: `node` + `npm`
- Templates: Need to provision Next.js templates (or fetch in CI)
- Test: `test_web_full_lifecycle`
- Evidence needed: Full create → install → test → build cycle

**AI Lifecycle:**
- Runtime: `python3`
- Test: Currently only tests create (needs expansion)
- TODO: Add full install → test cycle

**App Lifecycle:**
- Runtime: Flutter SDK
- Test: `test_app_full_lifecycle_limited`
- TODO: Expand from create-only to full cycle

### 3. Cache Tests (`cli/tests/cache_stress_test.rs`)

**All Cache Tests:**
- Runtime: `node` + `npm` (for Web dependency tests)
- Tests: All 5 cache tests use Web dependencies
- Evidence needed: Cache operations with real packages

### 4. CLI Tests (`cli/tests/cli_surface_errors_test.rs`)

**Minimal requirements:**
- Runtime: None (tests CLI surface only)
- Some tests benefit from Node for realistic scenarios

## Recommended CI Matrix

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
    include:
      - os: ubuntu-latest
        setup: |
          sudo apt-get update
          sudo apt-get install -y python3 python3-pip
          pip3 install pytest
          # Flutter install steps for Linux
          
      - os: macos-latest
        setup: |
          brew install python3
          pip3 install pytest
          # Flutter install via homebrew or direct download
          
      - os: windows-latest
        setup: |
          choco install python3 -y
          pip install pytest
          # Flutter install for Windows
```

## Current Test Status Without Runtimes

Running `cargo test --workspace` locally **WITHOUT** pytest/Flutter:

**Expected Results:**
- ✅ Security tests: 9/9 PASS
- ✅ Optimizer (Web, Lib): 2/2 PASS
- ❌ Optimizer (AI, App): 2 PANIC UNVERIFIED (pytest/Flutter missing)
- ✅ Lifecycle (Lib): 1 PASS
- ❌ Lifecycle (Web, App): 2 PANIC UNVERIFIED
- ⚠️ Cache tests: May PANIC if Node missing

**This is CORRECT** - tests failing when can't verify is honest reporting.

## Steps to Enable Full Verification

### Phase 1: Local Development (Optional)
Developers can install runtimes locally:
```bash
# macOS
brew install python3 node
pip3 install pytest
# Flutter: https://docs.flutter.dev/get-started/install/macos

# Linux
sudo apt-get install python3 python3-pip nodejs npm
pip3 install pytest
# Flutter: see official docs

# Windows
choco install python3 nodejs npm
pip install pytest
# Flutter: see official docs
```

### Phase 2: CI Provisioning (Required for Release)
Update `.github/workflows/ci.yml`:
1. Add runtime installation steps
2. Run full test suite with all runtimes
3. Report test results with breakdown (verified vs unverified)

### Phase 3: Template Provisioning (Web Tests)
Options:
1. Pre-fetch templates in CI setup
2. Mock template layer for tests
3. Include minimal templates in test fixtures

## Success Criteria

When CI is properly provisioned, expect:
- ✅ All optimizer tests: 4/4 PASS
- ✅ All lifecycle tests: 4/4 PASS (with full cycles)
- ✅ All cache tests: 5/5 PASS
- ✅ All CLI tests: 7/7 PASS

**Total**: ~30+ E2E tests fully VERIFIED (currently ~15 verified, ~15 unverified)

## Why This Matters

**Before**: Tests returned early (SKIP) → appeared to pass → false confidence  
**Now**: Tests panic UNVERIFIED → fail loudly → honest status  
**Future**: CI provisions runtimes → tests PASS with real evidence → verified confidence

This document explains WHY tests fail and HOW to make them pass.
The failures are CORRECT and EXPECTED without runtimes.
