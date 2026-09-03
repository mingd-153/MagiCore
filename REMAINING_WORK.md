# Remaining Work for Public Beta

Status: INTERNAL EXPERIMENTAL → PUBLIC BETA
Current blockers documented here.

## CRITICAL (Blocking Public Beta)

### 1. CI Runtime Provisioning ⚠️ IN PROGRESS
**Status**: CI workflow updated, needs testing  
**File**: `.github/workflows/ci.yml` (test-with-runtimes job added)  
**What**: Provision Python+pytest, Node+npm in CI  
**Expected**: AI optimizer tests PASS, cache tests PASS  
**Still need**: Flutter SDK, template provisioning  

**Action**: Push branch, run CI, verify pytest tests pass

### 2. Flutter Provisioning ✅ DONE
**Status**: Added to CI matrix  
**Implementation**: subosito/flutter-action@v2 in .github/workflows/ci.yml  
**What**: Flutter 3.24.0 stable installed on ubuntu + macos  
**Expected**: App optimizer + lifecycle tests will PASS in CI

**Action**: Test in CI after push

### 3. Template Provisioning ❌ TODO
**Status**: Not started  
**Blocker**: Web lifecycle test fails (templates not available)  
**Options**:
1. Pre-fetch templates in CI setup step
2. Include minimal test templates in fixtures
3. Mock template layer for tests

**Action**: Decide approach, implement

### 4. Real Distribution Artifacts ❌ TODO
**Status**: Placeholder hashes in manifests  
**Blocker**: Cannot brew/Scoop install until real artifacts built  
**What**: CI release workflow builds artifacts, computes SHA256, updates manifests  
**Current**: Manifests say "PLACEHOLDER_WILL_BE_REPLACED_BY_CI"  

**Action**:
1. Trigger release workflow (git tag + push)
2. Verify artifacts built for all 6 platforms
3. Verify update-release-hashes.sh replaces placeholders
4. Test brew install/Scoop install with real artifacts

### 5. Full Lifecycle Tests ❌ TODO
**Status**: Only Lib has full cycle, others partial  
**Web**: Only create (needs templates for install → build)  
**AI**: Only create (has pytest now, needs full cycle implementation)  
**App**: Only create (needs Flutter for build → test)  

**Action**: Expand test implementations for full create → install → build → test → run

## HIGH PRIORITY (Quality Improvements)

### 6. Comment Song Ngữ (RULE §7) ⚠️ AUDIT EXISTS
**Status**: Audit script checks, but not enforced  
**What**: Major functions need English + Vietnamese comments  
**Current**: Many English-only comments  

**Action**: Not blocking, but should improve for process compliance

### 7. Smoke Test Real Install ⚠️ SCRIPT EXISTS
**Status**: Script tests binary, not install process  
**What**: Actually test `brew install magicore` after release  
**Current**: Only tests `mgc --version` on local binary  

**Action**: Manual test after release, or add to CI post-release job

## MEDIUM PRIORITY (Nice to Have)

### 8. Cache Tests Stronger ✅ DONE
**Status**: Fixed - now requires both processes succeed  
**Previous**: Accepted 1/2 partial failure  

### 9. Quality Gate Strict ✅ DONE
**Status**: Fixed - audit now exits 1 on blocking issues  
**Previous**: Always exit 0  

### 10. Clippy Clean ❓ UNKNOWN
**Status**: Not run recently  
**Action**: Run `cargo clippy --workspace -- -D warnings`

## COMPLETED ✅

- [x] Security validator (path-aware, proven)
- [x] Test honesty (SKIP→panic UNVERIFIED)
- [x] Quality gates strict (audit, cache, smoke)
- [x] Whitespace clean (git diff --check)
- [x] Code formatted (cargo fmt --check)
- [x] Manifests honest (PLACEHOLDER text clear)

## Timeline Estimate

**If done sequentially**:
1. CI runtime test (1 day) - verify pytest provisioning works
2. Flutter + templates (2-3 days) - provision in CI
3. Full lifecycle impl (2 days) - expand Web/AI/App tests
4. Release artifacts (1 day) - trigger release, verify
5. Smoke test installs (1 day) - test brew/Scoop

**Total**: ~1 week of work (if parallelized, ~3-4 days)

**Critical path**: CI runtimes → Full tests → Release → Verify installs

## Decision Points

### Should App tests require Flutter?
**Options**:
A. Provision Flutter in CI (complete but heavy)
B. Mock Flutter adapter (faster but less real)
C. Mark App tests #[ignore] until Flutter CI ready

**Recommendation**: A (provision Flutter) for public beta confidence

### Should Web tests require templates?
**Options**:
A. Pre-fetch in CI (complete)
B. Include test fixtures (simpler)
C. Mock template layer (less real)

**Recommendation**: B (fixtures) for test speed, A (pre-fetch) for full E2E

### When to claim "PUBLIC BETA READY"?
**Minimum bar**:
- ✅ All quality gates pass
- ✅ CI runtime matrix includes pytest, Flutter, templates
- ✅ All E2E tests PASS (not UNVERIFIED)
- ✅ Real artifacts with real SHA256
- ✅ Tested brew/Scoop install on real artifacts

**Only after these 5 criteria met.**

## Current Status Summary

**Done**: 3/10 tasks fully verified  
**In progress**: 1/10 (CI runtime provisioning)  
**TODO**: 6/10 (Flutter, templates, full lifecycle, artifacts, installs, smoke)  

**Realistic status**: 30-40% done toward public beta  
**Agent claim**: Was "100% done" (incorrect)  
**User audit**: Exposed gap between infrastructure and verification  

**Next immediate action**: Test CI workflow with pytest provisioning
