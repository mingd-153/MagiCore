# C + Rust + Zig Architecture Plan

## 1. Tổng Quan

### Vấn đề hiện tại
| Issue | Root Cause | Severity |
|-------|-----------|----------|
| `mg install` mất 10 phút cho 677 packages | Resolver HTTP sequential + no cache | CRITICAL |
| Semver bug (next.9 > next.24) | String comparison thay vì numeric | ✅ FIXED |
| No build system | justfile không cross-compile được | MEDIUM |
| Tarball SHA-256 verify chậm | Rust sha2 crate overhead | LOW |
| Resolver + Installer không có progress | Thiếu observability | MEDIUM |

### Chiến lược 3 ngôn ngữ

```
┌─────────────────────────────────────────────────────────┐
│                     Zig (Build)                         │
│  build.zig · cross-compile · C test runner · codegen    │
├─────────────────────────────────────────────────────────┤
│                    Rust (Logic)                          │
│  resolver · installer · linker · CLI · store · tests    │
├─────────────────────────────────────────────────────────┤
│                   C (Performance)                        │
│  semver · JSON field extract · SHA-256 hot path         │
└─────────────────────────────────────────────────────────┘
```

**Nguyên tắc phân chia:**
- **Nhiệm vụ nào hot nhất** (gọi nhiều lần, loop tight) → C
- **Nhiệm vụ nào phức tạp, cần safety** → Rust
- **Nhiệm vụ nào build/tooling** → Zig

---

## 2. Language Responsibilities

### 2.1 C (`crates/mg-core-c/`)

**Vai trò:** Zero-overhead core operations, stack-only allocation, minimal dependencies.

**Files:**

```
crates/mg-core-c/
├── build.zig                  # Zig build for C library (tests only)
├── include/
│   ├── mg_semver.h            # Public API: version parse/compare/range
│   ├── mg_json.h              # Public API: JSON field extraction
│   └── mg_sha256.h            # Public API: SHA-256 streaming hash
├── src/
│   ├── semver.c               # ~150 lines
│   ├── json_extract.c         # ~200 lines (minimal, no full parse)
│   ├── sha256.c               # ~100 lines (wraps system crypto or builtin)
│   └── test/
│       ├── test_semver.c
│       ├── test_json.c
│       └── test_sha256.c
└── Makefile (optional, zig build thay thế)
```

#### 2.1.1 mg_semver.h — API

```c
#ifndef MG_SEMVER_H
#define MG_SEMVER_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

// ── Version ──
typedef struct {
    uint64_t major;
    uint64_t minor;
    uint64_t patch;
    // Pre-release: "alpha.1", "next.24", "rc.2"
    // Stored as dot-separated identifiers, e.g. "next.24"
    char prerelease[64];  // 63 chars max + null
    int prerelease_len;   // -1 if no prerelease
} mg_version_t;

// Parse "1.2.3" or "1.0.0-next.24" or "2.0.0+build.123"
// Returns 0 on success, -1 on error
int mg_version_parse(const char* s, mg_version_t* v);

// Compare two versions (per semver 2.0.0 spec)
// Returns: -1 if a < b, 0 if a == b, 1 if a > b
int mg_version_cmp(const mg_version_t* a, const mg_version_t* b);

// Format version back to string (max 127 chars + null)
int mg_version_format(const mg_version_t* v, char* out, size_t out_len);

// ── Version Range ──
// Unified struct for ^x.y.z, ~x.y.z, >=x.y.z, <x.y.z, x.y.z, *
typedef struct {
    enum {
        MG_RANGE_EXACT,    // "1.2.3"
        MG_RANGE_CARET,    // "^1.2.3"
        MG_RANGE_TILDE,    // "~1.2.3"
        MG_RANGE_GTE,      // ">=1.2.3"
        MG_RANGE_GT,       // ">1.2.3"
        MG_RANGE_LTE,      // "<=1.2.3"
        MG_RANGE_LT,       // "<1.2.3"
        MG_RANGE_STAR,     // "*"
        MG_RANGE_AND,      // ">=1.0.0 <2.0.0" (intersection)
        MG_RANGE_OR,       // "^1.0.0 || ^2.0.0" (union)
        MG_RANGE_INVALID,
    } type;
    // For simple ranges (EXACT, CARET, TILDE, GTE, GT, LTE, LT):
    mg_version_t min;
    mg_version_t max;  // exclusive upper bound for CARET/TILDE/AND
    // For AND: up to 2 sub-ranges (left/right)
    struct { struct mg_range_t left, right; } and_;
    // For OR: up to 2 sub-ranges (left/right)
    struct { struct mg_range_t left, right; } or_;
} mg_range_t;

// Parse range string into mg_range_t
// Returns 0 on success, -1 on error
int mg_range_parse(const char* s, mg_range_t* r);

// Returns true if range r contains version v
bool mg_range_contains(const mg_range_t* r, const mg_version_t* v);

#endif // MG_SEMVER_H
```

#### 2.1.2 mg_json.h — API

```c
#ifndef MG_JSON_H
#define MG_JSON_H

#include <stdbool.h>
#include <stddef.h>

// ── Lightweight JSON field extraction ──
// Intended for npm registry response parsing only.
// Does NOT implement full JSON — only flat field lookup.

// Extract a top-level string field value.
// Returns 0 on success, -1 if field not found or not a string.
// Fields: "name", "version", "description", "_id", "dist.tarball"
int mg_json_get_string(const char* json, const char* key, char* out, size_t out_len);

// Extract a top-level integer field value.
// Returns 0 on success, -1 if field not found or not an integer.
int mg_json_get_int(const char* json, const char* key, int* out);

// Callback for array/object iteration
typedef int (*mg_json_field_cb)(const char* key, size_t key_len,
                                const char* val, size_t val_len,
                                void* ctx);

// Iterate over all key-value pairs in an object
int mg_json_object_for_each(const char* json, mg_json_field_cb cb, void* ctx);

// Find the "versions" object in npm package metadata and iterate its keys
int mg_json_iterate_versions(const char* json, mg_json_field_cb cb, void* ctx);

// Find the "dependencies" object in a specific version entry and iterate
int mg_json_iterate_dependencies(const char* json, const char* version,
                                  mg_json_field_cb cb, void* ctx);

#endif // MG_JSON_H
```

#### 2.1.3 mg_sha256.h — API

```c
#ifndef MG_SHA256_H
#define MG_SHA256_H

#include <stddef.h>
#include <stdint.h>

#define MG_SHA256_HEX_SIZE 65  // 64 hex chars + null

// Streaming SHA-256 context (no heap allocation)
typedef struct mg_sha256_ctx mg_sha256_ctx_t;

// Initialize context
void mg_sha256_init(mg_sha256_ctx_t* ctx);

// Feed data
void mg_sha256_update(mg_sha256_ctx_t* ctx, const void* data, size_t len);

// Finalize and write hex digest to out (must be MG_SHA256_HEX_SIZE)
void mg_sha256_final_hex(mg_sha256_ctx_t* ctx, char* out);

// One-shot: hash data and write hex digest
void mg_sha256_hash(const void* data, size_t len, char* out);

#endif // MG_SHA256_H
```

#### 2.1.4 C Test Strategy

Mỗi file `.c` trong `src/test/` biên dịch thành binary chạy độc lập:

```bash
# Chạy C tests qua zig
zig build test-c

# Hoặc trực tiếp
cc -o test_semver src/test/test_semver.c src/semver.c && ./test_semver
```

Test cases:
- `test_semver.c`: So sánh với npm semver behavior (100+ test cases)
- `test_json.c`: Parse mẫu JSON từ npm registry response thật
- `test_sha256.c`: Verify với known test vectors (NIST SHA-256)

---

### 2.2 Rust (giữ lại + mở rộng)

**Vai trò:** Toàn bộ package manager logic, FFI binding, async I/O.

#### 2.2.1 FFI Binding Layer (mới)

**File:** `crates/mg-core/src/cffi.rs`

```rust
//! Safe wrappers around mg-core-c FFI functions
//!
//! Each C function is wrapped in a safe Rust function that:
//! 1. Validates input
//! 2. Calls C via unsafe
//! 3. Converts errors to Rust error types
//! 4. Manages memory/lifetimes

mod semver {
    /// Parse version string using C parser
    pub fn parse_version(s: &str) -> Result<Version, SemVerError> {
        // Calls mg_version_parse, wraps result in Rust Version
    }

    /// Compare versions using C implementation
    pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
        // Calls mg_version_cmp
    }

    /// Check range containment using C implementation
    pub fn range_contains(range: &VersionRange, version: &Version) -> bool {
        // Calls mg_range_contains
    }
}

mod json {
    /// Extract field from npm registry JSON response
    pub fn extract_string(json: &str, key: &str) -> Option<String> {
        // Calls mg_json_get_string
    }

    /// Iterate version keys from npm package metadata
    pub fn iterate_versions(json: &str) -> Vec<String> {
        // Calls mg_json_iterate_versions
    }
}
```

**Quy tắc an toàn:**
- `unsafe` block **tối thiểu**: chỉ gọi C function, càng nhỏ càng tốt
- Mọi C pointer được kiểm tra null trước khi dereference
- Stack allocation cho C structs (không heap allocate rồi pass)
- String buffer size fixed 128 bytes (đủ cho version, range)

#### 2.2.2 Registry Metadata Cache (mới)

**File:** `crates/mg-resolver/src/cache.rs`

```rust
use dashmap::DashMap;
use std::sync::Arc;

pub struct RegistryCache {
    // Key: package name (e.g., "react", "@types/node")
    // Value: cached JSON response + timestamp
    packages: DashMap<String, CachedEntry>,
    // TTL: 5 minutes
    ttl: Duration,
}

struct CachedEntry {
    json: Arc<serde_json::Value>,
    fetched_at: Instant,
}
```

**Cache policy:**
- Cache `get_package(name)` response per package name (không per version)
- TTL 5 phút (đủ cho 1 lần resolve)
- LRU eviction khi > 1000 entries
- Optional: persistent cache on disk (phase 2)

**Thay đổi trong resolver:**
```rust
// Before: mỗi call HTTP riêng
fn get_versions(&self, package: &PackageName) -> Vec<Version> {
    block_on(self.registry.get_package_versions(package))
}

// After: cache lookup
fn get_versions(&self, package: &PackageName) -> Vec<Version> {
    let json = self.cache.get_or_fetch(package, || {
        block_on(self.registry.get_package(package))
    });
    // Parse versions from cached JSON
    extract_versions_from_json(&json)
}
```

#### 2.2.3 Parallel Resolver HTTP (sửa)

**File:** `crates/mg-resolver/src/solver/mod.rs` (sửa `get_dependencies`)

```rust
// Before: block_on cho mỗi package
fn get_dependencies(&self, package_id: &PackageId) -> Vec<ResolvedDep> {
    tokio::runtime::Handle::current().block_on(async {
        let json = self.registry.get_package(package_id.name()).await;
        // ... parse dependencies from json
    })
}

// After: 
// 1. Cache lookup trước (tránh HTTP nếu đã có)
// 2. Batch những package chưa cache → spawn concurrent tasks
// 3. Dùng FuturesUnordered để chạy parallel
```

**Giải thích performance impact:**

Hiện tại `get_dependencies` được gọi ~150-300 lần trong 1 lần resolve. Mỗi lần gọi HTTP ~200-500ms (sequential). Total: **30-150 giây**.

Với cache: hit rate ~90% (cùng package name được query nhiều lần cho các version khác nhau). Chỉ ~15-30 lần HTTP thật sự.

Với parallel: 15-30 requests chạy đồng thời trong 1-2 batch → **~1-2 giây**.

**Estimated improvement: 30-150s → 1-2s** (15-75x improvement)

#### 2.2.4 Installer Pipeline Optimization (sửa)

**File:** `crates/mg-installer/src/installer/mod.rs`

Current issues:
1. **SQlite refcount insert sequential bottleneck** — mỗi task INSERT riêng, lock contention
2. **Tarball extraction semaphore held during whole task** — download + extract giữ semaphore, giảm effective concurrency

Fix:

```rust
// Fix 1: Batch SQLite inserts
// Thay vì mỗi task mở 1 connection và INSERT riêng:
// → Dùng channel, batch insert trong 1 transaction 100 records/lần

// Fix 2: Two-phase semaphore
// Phase 1: download semaphore (high concurrency, 32)
// Phase 2: extract + link semaphore (lower concurrency, 8)
// → Không block download trong khi extract

// Fix 3: Shared reqwest client with connection-per-host limit = 64
// (đã có, nhưng cần verify pool hoạt động đúng)
```

---

### 2.3 Zig

**Vai trò:** Build orchestration + cross-compilation + C codegen + testing.

#### 2.3.1 `build.zig` (thay thế justfile)

```zig
// build.zig
const std = @import("std");

pub fn build(b: *std.Build) void {
    // ── C Library ──
    const c_lib = b.addStaticLibrary(.{
        .name = "mg_core_c",
        .target = b.standardTargetOptions(.{}),
        .optimize = b.standardOptimizeOption(.{}),
    });
    c_lib.addCSourceFiles(.{
        .files = &c_src_files,
        .flags = &c_flags,
    });
    c_lib.addIncludePath(.{ .path = "crates/mg-core-c/include" });
    c_lib.linkLibC();

    // ── C Tests ──
    const c_test = b.addTest(.{
        .root_source_file = .{ .path = "crates/mg-core-c/test_runner.zig" },
        .target = target,
        .optimize = optimize,
    });
    c_test.linkLibrary(c_lib);

    // ── Rust via Cargo ──
    const cargo = b.addSystemCommand(&.{
        "cargo", "build", "--release", "-p", "mg-cli"
    });

    // ── Run all tests ──
    const test_all = b.step("test", "Run all tests (C + Rust)");
    test_all.dependOn(&c_test.step);
    test_all.dependOn(&b.addRunCommand(&.{"cargo", "test", "--workspace"}).step);

    // ── Cross-compilation ──
    // Dùng zig cc làm C compiler cho cargo
    // zig cc tự động cross-compile C code cho target bất kỳ
}
```

**Zig cung cấp:**
- `zig cc` làm C compiler (thay cc/clang) — cross-compile sẵn
- `zig build` chạy mọi thứ: C build + C test + Rust build + Rust test
- Cross-compilation targets không cần toolchain riêng
- Compile-time codegen cho templates

**Thay thế các justfile tasks:**
| justfile | build.zig | Status |
|----------|-----------|--------|
| `just build` | `zig build` | ✅ |
| `just test` | `zig build test` | ✅ |
| `just cross` | `zig build -Dtarget=...` | ✅ (built-in) |
| `just coverage` | `zig build coverage` | ✅ |
| `just audit` | `zig build audit` | ✅ |

#### 2.3.2 Compile-time Template Codegen

**File:** `crates/mg-scaffold/codegen.zig` (optional, phase 2)

Zig comptime có thể generate Rust source code cho templates:
```zig
// Thay thế handlebars runtime:
// Mỗi template được compile thành Rust code tại build time
// Zero runtime overhead, error tại compile time
```

---

## 3. Compiler Integration Strategy

### 3.1 Build Pipeline

```
                    ┌─────────────┐
                    │  zig build  │
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
      ┌───────────┐ ┌──────────┐ ┌──────────┐
      │ zig cc    │ │ cargo    │ │ zig test │
      │ (.c→.o)   │ │ (Rust)   │ │ (C tests)│
      └─────┬─────┘ └────┬─────┘ └──────────┘
            │            │
            ▼            ▼
      libmg_core_c.a ┌──────────┐
                     │ mg binary│
                     └──────────┘
```

### 3.2 Không Xung Đột

| Component | Compiler | Output | Notes |
|-----------|----------|--------|-------|
| C library | `zig cc` hoặc `cc` crate | `libmg_core_c.a` | Static lib, no PIC issues |
| C FFI tests | `zig test` | Test binary | Dùng same source |
| Rust crate | `rustc` via `cargo` | `mg` binary | Links C lib via build.rs |
| Rust tests | `cargo test` | Test binary | Same link step |

**`build.rs`** trong crate Rust (vd `mg-core/Cargo.toml`):

```rust
// crates/mg-core/build.rs
fn main() {
    let mut c = cc::Build::new();
    c.files(&[
        "crates/mg-core-c/src/semver.c",
        "crates/mg-core-c/src/json_extract.c",
        "crates/mg-core-c/src/sha256.c",
    ])
    .include("crates/mg-core-c/include")
    .compile("mg_core_c");
}
```

**Lưu ý:** `cc` crate tự động dùng C compiler mặc định của system (clang trên macOS). Khi cross-compile, nếu dùng `zig cc`, set `CC=zig cc` environment variable.

### 3.3 Cross-compilation

```bash
# Cross-compile với zig làm C compiler
CC="zig cc" \
TARGET_ARCH="aarch64-linux-gnu" \
cargo build --release --target aarch64-unknown-linux-gnu -p mg-cli

# Hoặc qua zig build
zig build -Dtarget=aarch64-linux-gnu
```

Zig's `zig cc` tự động:
- Chọn đúng target triple
- Link đúng libc cho target
- Không cần toolchain riêng

---

## 4. Refactoring Steps (Chi Tiết)

### Step 1: `build.zig` + Project Struct

**Files tạo mới:**
- `build.zig` tại root (~80 lines)
- `crates/mg-core-c/build.zig` (~30 lines)
- `crates/mg-core-c/include/mg_semver.h`
- `crates/mg-core-c/include/mg_json.h`
- `crates/mg-core-c/include/mg_sha256.h`

**Files sửa:**
- `Cargo.toml` (thêm `cc` dep cho mg-core, nếu chưa có)
- `crates/mg-core/build.rs` (compile C code)

**Test:** `zig build test` chạy được C test

### Step 2: C semver Implementation

**Files tạo mới:**
- `crates/mg-core-c/src/semver.c` (~150 lines)
- `crates/mg-core-c/src/test/test_semver.c` (~200 lines)

**Implementation details:**
- `mg_version_parse`: Parse "X.Y.Z", handle pre-release và build metadata
- `mg_version_cmp`: So sánh theo semver 2.0.0 spec (numeric so sánh numeric, string so sánh string, numeric < string)
- `mg_range_parse`: Support `^`, `~`, `>=`, `>`, `<=`, `<`, `*`, `||`, `&&`
- `mg_range_contains`: Chính xác như npm semver behavior (pre-release base check)

**Edge cases:**
- `1.0.0-next.24 > 1.0.0-next.9` ✅ (numeric comparison)
- `1.0.0 > 1.0.0-alpha` ✅ (release > pre-release)
- `^1.0.0-next.24` không chứa `1.0.0-next.9` ✅
- `^1.0.0` chứa `1.0.0` nhưng không chứa `2.0.0`
- `~1.2.0` chứa `1.2.9` nhưng không chứa `1.3.0`
- `>=1.0.0 <2.0.0` chứa `1.5.0` nhưng không chứa `2.0.0`
- `^1.0.0 || ^2.0.0` chứa cả `1.5.0` và `2.3.0`

### Step 3: Rust FFI Binding

**Files tạo mới:**
- `crates/mg-core/src/cffi/mod.rs` (re-exports)
- `crates/mg-core/src/cffi/semver.rs`
- `crates/mg-core/src/cffi/json.rs`
- `crates/mg-core/src/cffi/sha256.rs`

**Integration test:**
- `tests/cffi_test.rs`: So sánh kết quả C vs Rust cho 1000+ random inputs

### Step 4: Registry Cache

**Files tạo mới:**
- `crates/mg-resolver/src/cache.rs`

**Files sửa:**
- `crates/mg-resolver/src/lib.rs` (export cache module)
- `crates/mg-resolver/src/solver/mod.rs` (use cache in DependencyProvider)

**Performance test:**
- Benchmark: resolve 100 packages với cache vs không cache
- Mục tiêu: 95% cache hit rate

### Step 5: Parallel Resolver

**Files sửa:**
- `crates/mg-cli/src/main.rs` (async get_dependencies)
- `crates/mg-resolver/src/solver/mod.rs` (parallel batch fetch)

**Cách làm:**
1. Collect tất cả unique package names cần fetch
2. Dùng `FuturesUnordered` spawn concurrent HTTP requests
3. Map results về từng package ID

### Step 6: Install Pipeline Optimization

**Files sửa:**
- `crates/mg-installer/src/installer/mod.rs`

**Cụ thể:**
- Tách semaphore download (32) vs extract (8)
- Batch SQLite INSERT trong transaction
- Dùng channel thay vì semaphore (giảm tokio task overhead)

---

## 5. Testing Strategy

### 5.1 C Tests

```bash
# Run only C library tests
zig build test-c

# Test-driven: so sánh với reference npm output
zig build test-c -- --reference
```

**Test cases cho semver:**
```c
// test_semver.c
void test_prerelease_numeric_ordering() {
    mg_version_t a, b;
    mg_version_parse("1.0.0-next.9", &a);
    mg_version_parse("1.0.0-next.24", &b);
    assert(mg_version_cmp(&a, &b) == -1);  // a < b
}

void test_prerelease_string_ordering() {
    mg_version_t a, b;
    mg_version_parse("1.0.0-alpha", &a);
    mg_version_parse("1.0.0-beta", &b);
    assert(mg_version_cmp(&a, &b) == -1);  // alpha < beta
}

void test_release_vs_prerelease() {
    mg_version_t a, b;
    mg_version_parse("1.0.0", &a);
    mg_version_parse("1.0.0-alpha", &b);
    assert(mg_version_cmp(&a, &b) == 1);  // release > prerelease
}
```

### 5.2 Rust Tests

- **Unit tests**: mỗi module có `#[cfg(test)] mod tests`
- **FFI tests**: So sánh output C vs output Rust cho 1000+ phiên bản random
- **Integration tests**: `tests/integration/` — full pipeline test
- **E2E tests**: `tests/e2e/` — `mg install` với project thật

### 5.3 Zig Tests

- **Build system tests**: `zig build test` chạy được
- **C library tests** via `zig test`: test C code từ Zig
- **Cross-compilation tests**: Build cho 5 targets

### 5.4 Regression Tests (quan trọng)

**Test case @polka/url regression:**
```rust
#[test]
fn test_polka_url_regression() {
    let range = VersionRange::parse("^1.0.0-next.24").unwrap();
    let v9 = Version::parse("1.0.0-next.9").unwrap();
    let v24 = Version::parse("1.0.0-next.24").unwrap();
    let v29 = Version::parse("1.0.0-next.29").unwrap();
    
    assert!(!range.contains(&v9), "next.9 should NOT satisfy ^1.0.0-next.24");
    assert!(range.contains(&v24), "next.24 should satisfy ^1.0.0-next.24");
    assert!(range.contains(&v29), "next.29 should satisfy ^1.0.0-next.24");
}
```

**Fuzz testing:**
```
cargo fuzz run semver_roundtrip  # 1M random inputs
```

---

## 6. Risk Analysis

### 6.1 Risks & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| C semver không matching npm semver 100% | HIGH | HIGH | Test với 1000+ npm packages thật; so sánh với node-semver output |
| FFI undefined behavior (UB) | MEDIUM | CRITICAL | Minimal unsafe blocks; mỗi C function test riêng với valgrind/ASAN |
| `cc` crate không cross-compile được | LOW | MEDIUM | Dùng zig cc làm cross-compiler; fallback system cc cho native |
| Cache hit rate thấp | MEDIUM | MEDIUM | Measure real hit rate; adjust TTL |
| Zig version compatibility | LOW | LOW | Pin zig version trong build.zig.zon |
| Build time tăng (C compilation) | LOW | LOW | C files nhỏ (<200 lines mỗi file), compile ~0.1s |

### 6.2 Error Handling

**C layer errors:**
- `mg_version_parse` trả về -1 khi string không hợp lệ
- `mg_json_get_string` trả về -1 khi field không tồn tại
- Rust wrapper chuyển thành `Result<T, MgError>`

**Rust FFI safety:**
```rust
pub fn parse_version(s: &str) -> Result<Version, SemVerError> {
    let mut c_ver = mg_version_t::zeroed();  // Stack-allocated, zero-initialized
    let c_str = CString::new(s).map_err(|_| SemVerError::InvalidFormat(s.to_string()))?;
    let ret = unsafe { mg_version_parse(c_str.as_ptr(), &mut c_ver) };
    if ret != 0 {
        return Err(SemVerError::InvalidFormat(s.to_string()));
    }
    Ok(Version {
        major: c_ver.major,
        minor: c_ver.minor,
        patch: c_ver.patch,
        prerelease: if c_ver.prerelease_len >= 0 {
            let s = unsafe { std::ffi::CStr::from_ptr(c_ver.prerelease.as_ptr()) };
            Some(s.to_string_lossy().into_owned())
        } else {
            None
        },
        build: None,
    })
}
```

### 6.3 Benchmark Points

**Baseline (Rust-only, hiện tại):**
```
mg install (677 packages, cold cache):  594s wall, 22s CPU
Resolver time (estimate):                300-400s
Download time:                           170-200s
Extract + link time:                      20-30s
```

**Target (sau optimization):**
```
mg install (677 packages, cold cache):  <120s wall
Resolver time (cache + parallel):        <10s
Download time (16 concurrent):            <90s
Extract + link time (optimized):          <15s
```

**Benchmark methodology:**
1. `hyperfine './mg install'` — đo wall time
2. `cargo bench -p mg-bench` — đo từng component
3. `perf stat ./mg install` — CPU cycles, cache misses
4. So sánh `strace -c` trước/sau optimization

---

## 7. Implementation Order

```
Phase 1: Foundation (Zig + C)
├── T1: build.zig + project structure
├── T2: C semver library + tests
└── T3: Rust FFI bindings

Phase 2: Performance (Rust)
├── T4: Registry cache
├── T5: Parallel resolver HTTP
└── T6: Install pipeline optimization

Phase 3: Verification
├── T7: Full test suite
├── T8: Benchmark measurement
└── T9: Security audit
```

---

## 8. File Change Summary

### Files Created
```
web/mg/
├── build.zig                                    # ~80 lines
├── build.zig.zon                                # ~10 lines
└── crates/mg-core-c/
    ├── build.zig                                # ~30 lines
    ├── include/
    │   ├── mg_semver.h                          # ~80 lines
    │   ├── mg_json.h                            # ~40 lines
    │   └── mg_sha256.h                          # ~30 lines
    ├── src/
    │   ├── semver.c                             # ~150 lines
    │   ├── json_extract.c                       # ~200 lines
    │   ├── sha256.c                             # ~100 lines
    │   └── test/
    │       ├── test_semver.c                    # ~200 lines
    │       ├── test_json.c                      # ~100 lines
    │       └── test_sha256.c                    # ~50 lines
    └── test_runner.zig                          # ~20 lines
```

### Files Modified
```
web/mg/
├── crates/mg-core/
│   ├── Cargo.toml               (add cc dep, optional)
│   ├── build.rs                 (add C compilation)
│   └── src/
│       ├── cffi/mod.rs          (new module)
│       ├── cffi/semver.rs       (new)
│       ├── cffi/json.rs         (new)
│       └── cffi/sha256.rs       (new)
├── crates/mg-resolver/
│   ├── src/
│   │   ├── lib.rs               (add cache module)
│   │   ├── cache.rs             (new)
│   │   └── solver/mod.rs        (integrate cache)
├── crates/mg-installer/
│   └── src/installer/mod.rs     (optimize pipeline)
├── crates/mg-cli/
│   └── src/main.rs              (parallel resolver)
├── crates/mg-core/src/
│   ├── semver.rs                (optional: use C ffi fallback)
│   └── package.rs               (optional: use C range)
└── tests/
    ├── cffi_test.rs             (new)
    └── integration/             (update tests)
```

## 9. Success Criteria

- [ ] `cargo test --workspace` pass 100%
- [ ] `cargo clippy --workspace -D warnings` pass
- [ ] `zig build test` pass (C tests)
- [ ] `mg install (nuxt, 677 packages)` < 120s
- [ ] Resolver cache hit rate > 90%
- [ ] Semver C implementation match npm semver 100% (tested)
- [ ] No new unsafe UB (valgrind/ASAN clean)
- [ ] Cross-compile cho aarch64-linux x86_64-linux aarch64-macos x86_64-macos x86_64-windows
