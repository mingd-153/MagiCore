# 🚀 PHASE 2: PERFORMANCE OPTIMIZATION PLAN

**Goal:** 594s → 120s install time (5x improvement)  
**Status:** 🚧 IN PROGRESS  
**Start Date:** 2026-07-06

---

## 📊 CURRENT STATE (Phase 1 Complete)

### ✅ What's Already Done
```
✅ C hot paths implemented (semver, JSON, SHA-256)
✅ Registry cache structure exists (RegistryCache)
✅ Parallel prefetch implemented (prefetch_versions)
✅ Cache integration in RegistryDependencyProvider
```

### ⚠️ What's Not Optimized Yet
```
❌ Resolver doesn't call prefetch (sequential HTTP)
❌ No resolver-level batch strategy
❌ Installer uses single semaphore for download+extract
❌ SQLite inserts are per-package (lock contention)
❌ No HTTP connection pooling verification
```

---

## 🎯 OPTIMIZATION TARGETS

### Target Breakdown (from C-RUST-ZIG-PLAN.md)
```
Component           Current    Target    Gap       Strategy
────────────────────────────────────────────────────────────
Resolver            300-400s   <10s      30-40x    Cache + Parallel HTTP
Download            170-200s   <90s      2x        Parallel + connection pool
Extract + Link      20-30s     <15s      2x        Two-phase semaphore
────────────────────────────────────────────────────────────
TOTAL               594s       120s      5x        Combined optimizations
```

---

## 📋 IMPLEMENTATION CHECKLIST

### Task 1: Enable Resolver Prefetch ⏳
**Priority:** HIGH  
**Impact:** 30-40x resolver speedup  
**Effort:** 2-3 days

**Current Problem:**
- Resolver calls `get_versions()` sequentially for each package
- Each call = 1 HTTP request (~200-500ms)
- 150-300 calls = 30-150 seconds total

**Solution:**
```rust
// Before (in solver):
for each package:
    versions = provider.get_versions(package).await  // Sequential!

// After:
let all_packages = collect_unique_packages();
let results = provider.prefetch_versions(&all_packages).await;  // Parallel!
```

**Files to Modify:**
1. `crates/mg-resolver/src/solver/mod.rs`
   - Add `collect_needed_packages()` method
   - Call `prefetch_versions()` before resolution loop
   - Use cached results in `get_versions()`

2. `crates/mg-cli/src/main.rs`
   - Verify `RegistryDependencyProvider::prefetch_versions()` is used
   - Add timing logs to measure improvement

**Expected Result:**
- 150-300 sequential HTTP calls → 10-20 parallel batches
- Resolver time: 300-400s → 5-10s
- Cache hit rate: ~90% (same package queried multiple times)

**Acceptance Criteria:**
- [ ] Resolver calls prefetch before resolution
- [ ] Timing logs show <10s resolver time
- [ ] Cache hit rate >85%
- [ ] All tests passing

---

### Task 2: Installer Two-Phase Semaphore ⏳
**Priority:** MEDIUM  
**Impact:** 2x download+extract speedup  
**Effort:** 1 day

**Current Problem:**
```rust
// Current: One semaphore for entire operation
let _permit = download_semaphore.acquire().await;
download_tarball();  // 2-10s
extract_tarball();   // 0.5-2s
// Semaphore held for 2.5-12s total
```
**Issue:** Extract blocks download of other packages.

**Solution:**
```rust
// Phase 1: Download (high concurrency = 32)
let _permit = download_sem.acquire().await;
let tarball = download_tarball();
drop(_permit);  // Release immediately!

// Phase 2: Extract (low concurrency = 8)
let _permit = extract_sem.acquire().await;
extract_tarball(tarball);
link_to_nm();
```

**Files to Modify:**
1. `crates/mg-installer/src/installer/mod.rs`
   - Replace single semaphore with two: `download_sem` (32), `extract_sem` (8)
   - Split install task into download + extract phases
   - Release download semaphore before extract

**Expected Result:**
- Download parallelism: 8 → 32 concurrent
- Extract parallelism: 8 (unchanged, I/O bound)
- Total time: 20-30s → 10-15s

**Acceptance Criteria:**
- [ ] Two separate semaphores implemented
- [ ] Download phase releases permit early
- [ ] Extract phase uses separate semaphore
- [ ] Timing logs show improvement
- [ ] All tests passing

---

### Task 3: Batch SQLite Inserts ⏳
**Priority:** MEDIUM  
**Impact:** Reduce lock contention  
**Effort:** 1 day

**Current Problem:**
```rust
// Current: Each package opens connection and INSERTs
for package in packages:
    let conn = store.connection();
    conn.execute("INSERT INTO refcounts ...");  // Lock contention!
```
**Issue:** 677 packages = 677 separate transactions = lock thrashing.

**Solution:**
```rust
// Batch inserts in groups of 100
let (tx, rx) = mpsc::channel(100);

// Worker thread: batch inserts
tokio::spawn(async move {
    let mut batch = Vec::new();
    while let Some(pkg) = rx.recv().await {
        batch.push(pkg);
        if batch.len() >= 100 {
            store.batch_insert_refcounts(&batch);
            batch.clear();
        }
    }
    // Flush remaining
    if !batch.is_empty() {
        store.batch_insert_refcounts(&batch);
    }
});

// Main: send packages to worker
for package in packages:
    tx.send(package).await;
```

**Files to Modify:**
1. `crates/mg-store/src/store.rs`
   - Add `batch_insert_refcounts(&[PackageInfo])`
   - Use single transaction for batch

2. `crates/mg-installer/src/installer/mod.rs`
   - Create channel for refcount updates
   - Send to channel instead of direct insert
   - Spawn batch worker

**Expected Result:**
- Transactions: 677 → ~7 (100 per batch)
- Lock contention: reduced by ~100x
- SQLite time: 2-5s → 0.2-0.5s

**Acceptance Criteria:**
- [ ] Batch insert method implemented
- [ ] Channel-based worker pattern
- [ ] Single transaction per batch
- [ ] Timing logs show improvement
- [ ] All tests passing

---

### Task 4: Verify HTTP Connection Pool ⏳
**Priority:** LOW  
**Impact:** Ensure reqwest pool configured correctly  
**Effort:** 1 hour

**Current State:**
```rust
// NpmRegistry uses reqwest::Client
// Check: Is pool_max_idle_per_host set?
```

**Tasks:**
1. Check `NpmRegistry::new()` configuration
2. Verify `reqwest::Client` has:
   - `pool_max_idle_per_host(64)` or higher
   - `pool_idle_timeout(Duration::from_secs(90))`
3. Add connection pool metrics/logging

**Files to Check:**
1. `crates/mg-registry/src/npm.rs`

**Expected Result:**
- Confirm connection reuse (no SSL handshake overhead)
- Pool size sufficient for parallel requests

**Acceptance Criteria:**
- [ ] Connection pool configured
- [ ] Pool size ≥32
- [ ] Idle timeout ≥60s
- [ ] Metrics show connection reuse

---

## 📈 MEASUREMENT PLAN

### Before/After Benchmarks

**Test Case:** Install 677 packages (Nuxt.js project)

#### Baseline (Phase 1)
```
Total Time:        594s (estimated)
Resolver Time:     300-400s
Download Time:     170-200s
Extract+Link:      20-30s
```

#### Target (Phase 2)
```
Total Time:        <120s (5x improvement)
Resolver Time:     <10s (30-40x improvement)
Download Time:     <90s (2x improvement)
Extract+Link:      <15s (2x improvement)
```

### Measurement Commands
```bash
# Full install benchmark
time ./target/release/mg install

# Resolver-only benchmark
RUST_LOG=trace ./target/release/mg install 2>&1 | grep "\[TIMING\]"

# Extract timing breakdown
./target/release/mg install 2>&1 | \
  grep -E "resolve|download|extract" | \
  awk '{print $2, $NF}'
```

### Success Metrics
```
Metric                  Baseline    Target    Status
──────────────────────────────────────────────────────
Total install time      594s        <120s     ⏳
Resolver calls          150-300     10-20     ⏳
Cache hit rate          0%          >85%      ⏳
Parallel downloads      8           32        ⏳
SQLite transactions     677         ~7        ⏳
```

---

## 🔄 IMPLEMENTATION ORDER

### Week 1: Core Optimizations
```
Day 1-2:  Task 1 - Enable Resolver Prefetch
Day 3:    Task 2 - Two-Phase Semaphore
Day 4:    Task 3 - Batch SQLite Inserts
Day 5:    Task 4 - Verify HTTP Pool
```

### Week 2: Testing & Validation
```
Day 6-7:  Full benchmark testing
Day 8:    Bug fixes + edge cases
Day 9:    Documentation + PHASE2_COMPLETE.md
Day 10:   Final audit + performance report
```

---

## ⚠️ RISKS & MITIGATION

### Risk 1: Cache Correctness
**Risk:** Stale cache data causes wrong resolution  
**Mitigation:**
- TTL = 5 minutes (refresh during install unlikely)
- Cache key includes version info
- Evict stale entries on cache miss

### Risk 2: Parallel HTTP Overload
**Risk:** 32 concurrent requests overwhelm registry  
**Mitigation:**
- Start with 16 concurrent, increase if stable
- Add rate limiting if needed
- Respect registry rate limit headers

### Risk 3: SQLite Deadlock
**Risk:** Batch inserts cause deadlock  
**Mitigation:**
- Use separate connection for batch worker
- WAL mode for concurrent reads
- Retry logic with backoff

### Risk 4: Two-Phase Semaphore Race
**Risk:** Extract starts before download completes  
**Mitigation:**
- Await download future before extract
- Pass tarball data explicitly
- Add assertions in debug mode

---

## 📊 PHASE 2 DELIVERABLES

### Code Changes
- [ ] `crates/mg-resolver/src/solver/mod.rs` - Prefetch integration
- [ ] `crates/mg-installer/src/installer/mod.rs` - Two-phase semaphore
- [ ] `crates/mg-store/src/store.rs` - Batch insert method
- [ ] `crates/mg-registry/src/npm.rs` - Connection pool config

### Documentation
- [ ] `PHASE2_COMPLETE.md` - Implementation summary
- [ ] `PERFORMANCE_REPORT.md` - Before/after benchmarks
- [ ] `OPTIMIZATION_GUIDE.md` - How optimizations work

### Tests
- [ ] Update integration tests for parallel behavior
- [ ] Add benchmark tests (criterion)
- [ ] Verify cache correctness tests
- [ ] Load testing with 1000+ packages

---

## 🎯 ACCEPTANCE CRITERIA (Phase 2)

| Criterion | Target | Status |
|-----------|--------|--------|
| Total install time (677 packages) | <120s | ⏳ |
| Resolver time | <10s | ⏳ |
| Cache hit rate | >85% | ⏳ |
| Parallel downloads | 32 | ⏳ |
| SQLite batch size | 100 | ⏳ |
| All tests passing | 100% | ⏳ |
| No regressions | 0 | ⏳ |

**Phase 2 Complete When:**
- All 7 criteria met
- Performance report generated
- Documentation updated
- Ready for Phase 3 (Production)

---

**Status:** 🚧 Task 1 starting next  
**Next Update:** After Task 1 complete (prefetch integration)
