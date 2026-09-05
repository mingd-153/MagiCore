# RC-1 Issues and RC-2 Remediation Plan

**Date**: 2026-09-05
**Status**: RC-1 has critical issues, preparing RC-2

---

## P0 Issues in v1.1.0-rc.1

### 1. GitHub Release Does Not Exist
- **Issue**: Tag exists but no Release created
- **Cause**: Workflow builds failed (4/6 targets failed)
- **Impact**: Users cannot install RC-1
- **Status**: BLOCKING

### 2. Invalid Performance Claims in Tag Annotation
Tag annotation contains:
```
✅ "26x faster cold install than pnpm" (2.43s vs 62.97s)
✅ Statistical rigor: 139 runs across 5 PMs
```

**Reality** (strict validation):
- Valid samples: 102 (not 139)
- mgc: 21 samples (not 44)
- bun: 5 samples (not 23)
- High CV: mgc 229%, npm 105%, yarn 156%
- Dataset has variance/mixing issues

**Impact**: Cannot defend claims publicly
**Fix**: RC-2 will NOT include performance claims until clean benchmark

### 3. Workflow Build Failures
Failed targets:
- aarch64-apple-darwin (macOS ARM64)
- aarch64-unknown-linux-gnu (Linux ARM64)
- aarch64-pc-windows-msvc (Windows ARM64)
- x86_64-pc-windows-msvc (Windows x64)

Success:
- x86_64-unknown-linux-gnu (Linux x64) ✅

**Status**: Awaiting logs to debug

### 4. Benchmark Data Quality
- Mixed workloads (11, 19, 20 packages)
- Invalid JSON in some results
- Timeout/hang runs (>1h)
- No workload_id, cache_state, exit_code in schema
- Silent rejection of malformed data

**Fix**: Created analyze_results_strict.py with validation

---

## Tasks Completed (3/10)

✅ **Task 2**: Cleaned 10 root files (session artifacts)
✅ **Task 3**: Fixed trailing whitespace (local only)
✅ **Task 4**: Created strict analyzer with schema validation

---

## Tasks Remaining (7/10)

### Blocking RC-2

**Task 1**: Check workflow logs when complete
- **Status**: Workflow still running (queued)
- **Action**: Wait for completion, extract failure logs
- **ETA**: Unknown (macOS x64 job queued)

**Task 7**: Fix release workflow
- **Dependency**: Task 1 (need logs)
- **Action**: Fix root cause for ARM64/Windows builds
- **Likely issues**: Cross-compile, esbuild-rs Go bindings

**Task 8**: Standardize artifact naming
- **Issue**: Inconsistent names across workflow/Homebrew/Scoop
- **Current**: `magicore-1.1.0-rc.1-macos-aarch64.tar.gz` vs `magicore-macOS-X64.tar.gz`
- **Target**: `magicore-{version}-{os}-{arch}.{ext}`

**Task 10**: Create RC-2 tag and verify Release
- **Dependency**: Tasks 1, 7, 8
- **Action**: New tag (do NOT move RC-1)
- **Verify**: `gh release view v1.1.0-rc.2` returns artifacts

### Non-Blocking (Can defer)

**Task 5**: Re-run clean benchmark
- **Scope**: 10 runs minimum per PM, controlled conditions
- **Goal**: CV <100%, consistent workload
- **Timeline**: Can do post-RC-2 release

**Task 6**: Remove performance claims
- **Status**: PARTIAL
- **Done**: README/CHANGELOG clean
- **Issue**: RC-1 tag annotation has claims (cannot edit)
- **Action**: RC-2 will not include claims until validated

**Task 9**: Test RC-2 artifacts
- **Dependency**: Task 10
- **Action**: Download from Release, verify checksums, run mgc --version/install

---

## RC-2 Requirements (Minimum Viable)

### Must Have
1. ✅ GitHub Release created (not just tag)
2. ✅ At least 2 platform builds succeed:
   - Linux x64 (already passes) ✅
   - macOS x64 (likely to pass - same arch as runner)
   OR
   - Windows x64 (fixed hardlink_tree)
3. ✅ Artifacts downloadable and runnable (`mgc --version` works)
4. ✅ No performance claims in tag/release notes
5. ✅ Quality gates pass (no trailing whitespace, no root files)

### Known Limitations (Document in Release Notes)
- **ARM64 targets not supported in RC-2** due to esbuild-rs Go dependency
  - macOS ARM64: esbuild requires Go compiler
  - Linux ARM64: OpenSSL cross-compile issues
  - Windows ARM64: Compile OK but untested
- **Workaround**: Use x86_64 builds with Rosetta 2 (macOS) or emulation
- **Future**: RC-3 will address with optional bundler feature or pre-built binaries

### Nice to Have (can defer to RC-3)
- All 6 platforms build
- Clean benchmark with CV <100%
- Performance claims validated
- Homebrew/Scoop tested

---

## RC-1 Tag Annotation (Cannot Change)

Tag annotation is immutable and contains invalid claims:
```
v1.1.0-rc.1 (2026-09-05)
...
Cold Install Rankings:
1. mgc: 2.43s (FASTEST)
2. bun: 47.36s (19x slower)
...
5. pnpm: 62.97s (26x slower)
...
✅ "26x faster cold install than pnpm" (2.43s vs 62.97s)
✅ Optimized for CI/CD pipelines (cold-start performance)
✅ Statistical rigor: 139 runs across 5 PMs
```

**These claims are NOT defensible** due to:
- High CV (229-346%)
- Mixed workloads
- Invalid sample counting

**Mitigation**: 
- Mark RC-1 as "do not use" in release notes
- RC-2 will have clean tag annotation
- Document issues transparently

---

## Next Immediate Actions

1. **Wait for workflow completion** (~10-30 min)
2. **Extract failure logs** for 4 failed jobs
3. **Identify root cause** (likely cross-compile or esbuild-rs)
4. **Fix workflow** for at least 3 platforms
5. **Create RC-2 tag** with clean annotation
6. **Verify GitHub Release** created with artifacts

---

## Lessons for RC-2

### Do
- ✅ Validate data before claiming results
- ✅ Use strict analyzer to count samples
- ✅ Report rejections transparently
- ✅ Test artifacts from GitHub Release (not local files)
- ✅ Standardize naming early
- ✅ Run quality gates before commit

### Don't
- ❌ Count files as valid samples
- ❌ Mix workloads without filtering
- ❌ Skip schema validation
- ❌ Make claims without CV check
- ❌ Commit .md docs to git (per RULE)
- ❌ Force-move release tags

---

**Document created**: 2026-09-05 02:30
**Workflow status**: Running (4 failed, 1 success, 1 queued)
**Next milestone**: Workflow completion + log extraction
