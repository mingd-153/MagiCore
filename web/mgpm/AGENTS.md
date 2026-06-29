# MGPM Session Context

> File này lưu trạng thái session để không mất context giữa các lần làm việc.
> Cập nhật sau mỗi phiên làm việc.

---

## Branch Strategy (Luồng Git Bắt Buộc)

```
main (production — chỉ CI/CD + release)
  ↑ merge khi release tag
development (nhánh dev chính — sạch sẽ, ít commits)
  ↑ merge cuối mỗi tuần
week-N (branch chung tuần N — tổng hợp tasks, N = 1,2,3...)
  ↑ merge từng task khi hoàn thành
feat-T<ID>-<slug> (task branch — làm xong xoá)
```

**Nguyên tắc:**
- `feat-*` → `week-*` → `development` → `main`
- Không nhảy cóc. Không merge thẳng vào `development`
- `development` chỉ nhận merge từ `week-*`
- `main` chỉ nhận merge từ `development` khi release
- Git không cho phép tên branch dạng `development/week-1` nếu đã có `development` → dùng `week-1`, `week-2`

## Development Loop (Vòng Lặp Bắt Buộc)

Mỗi task phải qua đủ vòng lặp này, không skip, không bịa:

```
create → check → run → fix → update → fix → done → report → push
```

| Bước | Hành động | Kiểm tra |
|:----:|-----------|----------|
| **create** | Tạo branch từ `development/week-N`: `git checkout -b feat-T<ID>-<slug>` | Đúng tên branch |
| **check** | Đọc file task, hiểu requirements + design | Không bịa, không phỏng đoán |
| **run** | Implement code + chạy `cargo check --workspace` | 0 lỗi compile |
| **fix** | Sửa lỗi clippy, test fail | `cargo clippy --workspace -D warnings` pass |
| **update** | Update `AGENTS.md` + doc comments | Ghi lại đúng trạng thái |
| **fix** | Chạy lại toàn bộ test | `cargo test --workspace` 100% pass |
| **done** | Verify checklist trong task file | Tất cả ✅ |
| **report** | Báo cáo kết quả thực tế cho user | Không bịa, không ảo, không skip |
| **push** | User approve → `git push` | Chỉ push sau khi user duyệt |

**Luật bất di bất dịch:**
- ❌ Không push nếu chưa qua vòng lặp đầy đủ
- ❌ Không bịa kết quả test, không bịa số liệu
- ❌ Không skip step nào trong vòng lặp
- ✅ Pass hết mới được báo cáo
- ✅ User duyệt mới được push

---

## Current Status

**Phase**: 0 — Foundation (Tuần 1-4)
**Current task**: T0.2 — ✅ CAS I/O complete
**Branch hiện tại**: `development`
**Branch gốc**: `development`
**Remote**: `https://github.com/mingd-153/MegaGate.git`
**Tests**: 304 passed (16 cas + 81 sqlite + others), 0 failed, 0 warnings

## What's Been Done

### T0.1 — SQLite Store Security Hardening (Uncommitted)

Đã fix toàn bộ 5 HIGH issues còn lại từ security audit:

| Issue | Mức | Fix | File |
|-------|:---:|-----|------|
| H2: Deep integrity check | HIGH | `deep_integrity_check()` dùng `PRAGMA integrity_check`, audit gọi cả quick+deep | `lifecycle.rs`, `audit.rs` |
| H3: Path traversal (register_project) | HIGH | Canonicalize path nếu tồn tại, reject `..`, cấm empty path | `store.rs` |
| H4: In-memory 512MB limit | HIGH | `PRAGMA max_page_count = 131072` (~512MB) | `lifecycle.rs` |
| H6: TOCTOU integrity cache | HIGH | `update_integrity_cache` rehash file content trước khi store | `store.rs` |
| H7: Algorithm validation | HIGH | `validate_algorithm()` chỉ cho phép sha256/blake3 | `store.rs` |

### Summary
- **81 SQLite tests, 288 total** — all pass
- **0 compile errors** (`cargo check --workspace`)
- **5/5 HIGH issues fixed**
- **3 Critical + 7 High** = 10 security fixes total (all resolved)

### T0.2 — CAS I/O Complete ✅

| Feature | Implementation | Security |
|---------|----------------|----------|
| `import_file` / `import_bytes` | Atomic `create_new` + verify same fd | TOCTOU eliminated |
| `export_to` | Hardlink preferred → verify hash | Symlink check dest |
| `verify` | Re-hash file | Content integrity |
| `contains` / `remove` | Index + disk check | Path traversal safe |
| `import_tarball_entries` | Batch import | SHA-256 dedup |
| Executable files | `-exec` suffix + `0o111` | Preserved on export |

**Security fixes applied:**
- Export destination symlink check (`check_symlink_ancestors` - path + parent only)
- Import source symlink check
- CAS root validation (not symlink) + `0o700` permissions
- TOCTOU fix: verify content using **same file handle** (read+seek+hash)
- `SystemTime` panic fix: `unwrap_or_default()`
- Path traversal impossible (SHA-256 hex only)

**Tests**: 16 CAS tests + 81 SQLite = 97 store tests, **304 total workspace tests pass**

**Benchmarks (Apple Silicon, release):**
| Operation | Time | Throughput |
|-----------|------|------------|
| import 1KB | 26.5 ms | 37.8 KiB/s |
| import 100KB | 27.1 ms | 3.6 MiB/s |
| import 1MB | 34.1 ms | 29.3 MiB/s |
| import 10MB | 132 ms | 75.9 MiB/s |
| export 1KB | 17.4 ms | 56 KiB/s |
| export 1MB | 35 ms | 28.6 MiB/s |
| verify 1KB | 1.8 ms | 560 KiB/s |
| verify 10MB | 23 ms | 435 MiB/s |
| concurrent 8 threads | 28 ms / 1K ops | ~285 op/s/thread |
| tarball batch 1000 | 66 ms | 15K elem/s |

### Phase 0 — Foundation (Tuần 1)

| # | Task | Files | Status |
|---|------|-------|--------|
| T0.1 | SQLite store: adaptive + KV + audit + permission monitor | `sqlite.rs` (→1050 dòng) | ✅ Expanded + Hardened |
| T0.2 | CAS import/export | `cas.rs` (new) + bench | ✅ Complete + Benchmarked |
| T0.4 | Lockfile integrity fix | — | ⏳ |
| 5 | Fuzz CI | `.github/workflows/fuzz.yml` | ✅ |
| 6 | Signed releases | `.github/workflows/release.yml` | ✅ |
| 7 | SECURITY.md | `SECURITY.md` (root) | ✅ |
| 8 | Sandbox module (stub) | `sandbox/` | ⚠️ Stub |
| 9 | TUF framework (stub) | `tuf.rs` | ⚠️ Stub |
| 10 | Dependency confusion check | `solver/mod.rs` | ✅ |
| 11 | `mgpm config` command | `main.rs` | ✅ |

## T0.1 Evaluation — Đánh Giá Tổng Quan

### 1. ⚡ Tốc Độ (Benchmark Results)

| Operation | Time | Notes |
|-----------|:----:|-------|
| **Open: create new** | 1.57 ms | Schema init + pragmas + health check |
| **Open: existing** | 411 µs | Cold start |
| **Open: readonly** | 277 µs | Lightweight |
| **Open: in-memory** | 215 µs | Fastest |
| **Bulk add: 100** | 2.20 ms | ~22 µs/pkg trong transaction |
| **Bulk add: 500** | 5.03 ms | ~10 µs/pkg |
| **Bulk add: 1000** | 8.64 ms | ~8.6 µs/pkg |
| **Query: by_name** | 5.56 µs | Index scan |
| **Query: by_integrity (cache)** | 167 ns | LRU hit |
| **Exists: cache hit** | 14 ns | Instant |
| **Exists: cache miss** | 2.18 µs | SQL query |
| **KV set: 1KB ×100** | 5.92 ms | ~59 µs/op |
| **KV get: ×100** | 347 µs | ~3.5 µs/op |
| **Health check** | 30 µs | PRAGMA quick_check |
| **Vacuum** | 388 µs | Quick |
| **Concurrent: 4 threads** | 5.97 ms | 4×100 queries |
| **ContentStore import: 100 files** | 27.3 ms | ~273 µs/file (baseline) |

**Verdict:** SQLite nhanh hơn file-based ContentStore khoảng 10-50× cho query operations. Bulk insert đạt ~8.6 µs/pkg.

### 2. 🔒 Bảo Mật (Security Audit) — ALL FIXED

| Check | Status | Notes |
|-------|:------:|-------|
| SQL injection | ✅ Safe | All queries use `?1` parameterized — không thể inject qua package name/version |
| `trusted_schema=OFF` | ✅ | Chống schema-based SQL injection |
| `busy_timeout` | ✅ | 5000ms — không treo vô hạn |
| File permissions | ✅ | Store được snapshot + monitor qua `audit()` |
| File permissions 0o600 | ✅ | DB file chỉ owner read/write |
| Permission override detect | ✅ | `check_permissions()` so sánh mode/size/mtime |
| BEGIN IMMEDIATE | ✅ | Tránh deadlock concurrent write |
| READONLY mode | ✅ | Open với flag readonly khi không cần ghi |
| WAL crash safety | ✅ | survive SIGKILL, `synchronous=NORMAL` |
| **Supply chain (C1)** | ✅ | `add_package` rejects integrity collision |
| **File permissions (C2)** | ✅ | DB file `0o600` sau open |
| **Unsafe FFI (C3)** | ✅ | Replaced libc::sysctl + kernel32 with sysinfo crate |
| **Cache poisoning (H1)** | ✅ | `get_cached_integrity` re-verify file content |
| **Deep integrity (H2)** | ✅ | `PRAGMA integrity_check` + quick_check trong audit |
| **Path traversal (H3)** | ✅ | `register_project` canonicalize + reject `..` |
| **Memory limit (H4)** | ✅ | In-memory DB max 512MB |
| **TOCTOU (H6)** | ✅ | `update_integrity_cache` rehash file content |
| **Algorithm validation (H7)** | ✅ | Only sha256/blake3 allowed |

**Nguy cơ còn lại (thấp):**
- Mutex poisoning: `unwrap()` khi lock — nếu thread panic sẽ poison, nhưng pattern chuẩn của Rust

### 3. 🛡 An Toàn (Safety Review)

| Check | Status | Notes |
|-------|:------:|-------|
| WAL mode | ✅ | Atomic commit, survive crash |
| `synchronous=NORMAL` | ✅ | Balance durability/speed |
| Transaction support | ✅ | BEGIN/COMMIT/ROLLBACK đầy đủ |
| Auto WAL checkpoint | ✅ | Khi WAL > 4000 pages |
| Integrity check | ✅ | `PRAGMA quick_check` trên mỗi open |
| Schema migration | ✅ | versioned, không break backward |
| File change detection | ✅ | `audit()` phát hiện file mới/mất/thay đổi |
| Stale audit warning | ✅ | Cảnh báo nếu >24h không audit |

### 4. 🧬 Thích Nghi (Adaptability)

| Check | Status | Notes |
|-------|:------:|-------|
| RAM detection (macOS) | ✅ | `sysctl HW_MEMSIZE` |
| RAM detection (Linux) | ✅ | `/proc/meminfo` |
| RAM detection (Windows) | ⚠️ | Fallback 2GB, cần `GlobalMemoryStatusEx` |
| Adaptive cache_size | ✅ | 2000 → 512000 pages (6 tiers) |
| Adaptive mmap_size | ✅ | 0 → 256MB (5 tiers) |
| Adaptive LRU size | ✅ | 1000 → 100000 entries |
| Readonly mode | ✅ | Lightweight pragmas, no table init |

### 5. 🔧 Mở Rộng (Extensibility)

| Check | Status | Notes |
|-------|:------:|-------|
| Schema migration | ✅ | `schema_version` table + versioned SQL |
| KV escape hatch | ✅ | `set_kv/get_kv/delete_kv` — zero schema |
| JSON metadata columns | ✅ | `metadata TEXT` on packages, projects, deps |
| StoreIndex trait | ✅ | Public trait, có thể implement backend khác |
| Plugin hooks | ⚠️ | Chưa có — cần observer pattern cho store events |
| Public API | ✅ | `SqliteStore::open()`, `audit()`, `health_check()` |

### 6. 🚨 Permission Monitoring (Tính năng mới)

Đã implement hệ thống giám sát permission cho store:

```
SqliteStore::audit()
  ├── integrity_check → PRAGMA quick_check
  ├── permission_check → snapshot diff (mode, size, mtime)
  ├── stale_check → >24h → cảnh báo
  └── metrics → db_size, wal_size, cache_entries, ram

SqliteStore::check_permissions()
  └── So sánh permission snapshot hiện tại vs snapshot cũ
      ├── mode change detect: "permission changed: file was 644 now 755"
      ├── mtime change detect: "file modified: index.db"
      ├── new file detect: "new file detected: index.db-wal"
      └── deleted file detect: "file removed: index.db-shm"

SqliteStore::snapshot_permissions()
  └── Lưu snapshot vào kv_store["permission_snapshot"]
```

AuditReport.is_healthy() = passed && integrity_ok && permissions_ok && !stale_warning

### Benchmark Summary (SQLite vs ContentStore)

```
Query:   SQLite 5.56µs     vs ContentStore ~273µs  → ~50× nhanh hơn
Bulk:    SQLite 8.6µs/pkg  vs ContentStore ~273µs/file → ~30× nhanh hơn
Open:    SQLite 1.57ms     vs ContentStore (N/A)   → embedded, không cần load index
Memory:  SQLite LRU cache  → adaptive 1k-100k entries
```

## Architecture Decisions

- **Store**: SQLite-backed (sẽ làm ở T0.1)
- **Linker**: 2 modes — hoisted (default) + isolated (future)
- **Lockfile**: TOML + bincode dual format
- **Scaffold**: Modular installer system (T2.2)
- **Monorepo**: mgpm.yaml config (T3.1)

## Key Files Reference

| File | Purpose | Lines |
|------|---------|:-----:|
| `crates/mgpm-cli/src/main.rs` | Main CLI (16 commands) | 2568 |
| `crates/mgpm-store/src/store/content_store.rs` | Current flat store | ~160 |
| `crates/mgpm-lockfile/src/lockfile/mod.rs` | Lockfile struct + integrity | 277 |
| `crates/mgpm-lockfile/src/pipeline.rs` | Resolution→lockfile pipeline | 115 |
| `crates/mgpm-resolver/src/solver/mod.rs` | Resolver + dep confusion check | 367 |
| `crates/mgpm-core/src/config.rs` | All config structs | 354 |
| `docs/ARCHITECTURE-V2.md` | Full architecture plan | ~1500 |
| `docs/SECURITY-REPORT.md` | Security audit + comparison | ~3000 |
| `tasks/README.md` | Task list (44 tasks) | — |

## Next Steps

Task còn lại trong Phase 0:
1. **T0.1**: ✅ Hoàn thành (expanded + security hardened — 10 vulnerabilities fixed)
2. **T0.2**: CAS content-addressed import/export ← tiếp theo
3. **T0.3**: Store verify + status
4. **T0.4**: Lockfile integrity fix - BLAKE3 + real SHA-256
5. **T0.5**: Global Virtual Store
6. **T0.6**: Isolated linker
7. **T0.7**: Integration test

## How to Continue

```bash
# 1. Đọc task chi tiết
cat tasks/phase-0-foundation/T0.2-cas-io.md

# 2. Tạo branch task từ development
git checkout development
git checkout -b feat-T0.2-cas-io

# 3. Implement theo vòng lặp
cargo check --workspace
cargo test --workspace
cargo clippy -p mgpm-store

# 4. Commit local + merge vào week-1
git add -A
git commit -m "feat(mgpm): T0.2 - CAS import/export"
git checkout week-1 || git checkout -b week-1
git merge feat-T0.2-cas-io
git branch -d feat-T0.2-cas-io

# 5. Update AGENTS.md
```

## Folder Structure Hiện Tại (Chi Tiết)

```
web/mgpm/
├── Cargo.toml                          # Workspace root (16 crates)
├── rust-toolchain.toml                 # Rust 1.84+
├── justfile                            # Task runner commands
├── install.sh                          # Bootstrap installer
├── mgpm.asc                            # GPG public key (placeholder)
├── AGENTS.md                           # Session context (file này)
│
├── crates/                             # ─── RUST CRATES ───
│   ├── mgpm-core/                      # Types, config, semver, errors
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Public exports
│   │       ├── config.rs              # MgpmConfig, RegistryConfig
│   │       ├── semver.rs              # Version parsing
│   │       ├── error.rs               # Error types
│   │       ├── alloc.rs               # Allocator config
│   │       └── logging.rs             # Logging setup
│   │
│   ├── mgpm-store/                     # Content-addressed store
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── store/
│   │           ├── mod.rs
│   │           ├── content_store.rs   # Current flat store
│   │           ├── tarball.rs         # Tarball extraction
│   │           └── cache.rs           # Package cache
│   │
│   ├── mgpm-registry/                  # Registry clients
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── registry/
│   │           ├── mod.rs             # RegistryClient trait
│   │           ├── npm.rs            # npm registry
│   │           ├── jsr.rs            # JSR registry
│   │           ├── git.rs            # Git tarball
│   │           ├── file.rs           # Local file
│   │           └── http.rs           # Generic HTTP
│   │
│   ├── mgpm-resolver/                  # PubGrub resolver
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── solver/
│   │           ├── mod.rs            # Resolver + dep confusion
│   │           └── pubgrub.rs        # PubGrub algorithm
│   │
│   ├── mgpm-lockfile/                  # Lockfile I/O
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── lockfile/
│   │       │   ├── mod.rs            # Lockfile struct
│   │       │   └── lockfile_v2.rs    # (future) v2 format
│   │       ├── binary.rs             # Bincode format
│   │       ├── text.rs               # TOML format
│   │       └── pipeline.rs           # Resolution->lockfile
│   │
│   ├── mgpm-installer/                 # Install pipeline
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── installer/
│   │           └── mod.rs            # Install logic
│   │
│   ├── mgpm-linker/                    # node_modules linker
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── linker/
│   │           └── mod.rs            # Current hoisted linker
│   │
│   ├── mgpm-plugins/                   # napi-rs plugins
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── plugins/
│   │           ├── mod.rs
│   │           └── builtin/          # Built-in plugins
│   │
│   ├── mgpm-workspace/                 # Workspace management
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs                # Workspace discovery
│   │
│   ├── mgpm-cli/                       # CLI (clap)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs               # 2568 lines, 16 commands
│   │       ├── lib.rs                # Re-exports
│   │       ├── advisory.rs           # OLD (dead code)
│   │       ├── advisory_db.rs        # Advisory DB with remote fetch
│   │       ├── tuf.rs                # TUF (currently stub)
│   │       ├── auth.rs              # Auth hardening
│   │       ├── profiler.rs           # CPU/memory profiler
│   │       └── sandbox/
│   │           ├── mod.rs            # Sandbox module
│   │           ├── macos.rs          # macOS (stub)
│   │           ├── linux.rs          # Linux (stub)
│   │           └── macos.sb          # Seatbelt profile
│   │
│   ├── mgpm-bench/                     # Criterion benchmarks
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs
│   │   └── benches/
│   │       ├── store.rs
│   │       ├── resolver.rs
│   │       └── lockfile.rs
│   │
│   └── (future crates)
│       ├── mgpm-scaffold/             # (T2.1)
│       ├── mgpm-monorepo/             # (T3.1)
│       ├── mgpm-cache/                # (T1.2)
│       ├── mgpm-security/             # (T4.1)
│       ├── mgpm-script/               # (T4.8)
│       ├── mgpm-sandbox/             # (T4.9)
│       └── mgpm-daemon/              # (T5.2)
│
├── fuzz/                               # Fuzz targets
│   ├── Cargo.toml
│   └── targets/
│       ├── lockfile_parse.rs
│       └── registry_response.rs
│
├── tests/                              # Integration tests
│
├── docs/                               # Documentation
│   ├── ARCHITECTURE-V2.md             # Full architecture plan
│   ├── SECURITY-REPORT.md             # Security audit + comparison
│   ├── book.toml                      # mdBook config
│   └── src/                           # mdBook source
│       ├── SUMMARY.md
│       ├── introduction.md
│       └── getting-started.md
│
├── tasks/                              # Task files (gitignored)
│   ├── README.md                      # 44 tasks overview
│   ├── TEMPLATE.md
│   ├── phase-0-foundation/            # 7 tasks
│   ├── phase-1-speed/                 # 8 tasks
│   ├── phase-2-scaffolding/           # 13 tasks
│   ├── phase-3-monorepo/              # 9 tasks
│   ├── phase-4-security/              # 10 tasks
│   ├── phase-5-optimization/          # 7 tasks
│   └── phase-6-ecosystem/             # 7 tasks
│
├── templates/                          # Scaffolding templates (future)
│   ├── vanilla/
│   ├── react/
│   ├── next/
│   ├── vue/
│   ├── express/
│   ├── fastify/
│   ├── node-lib/
│   ├── cli/
│   └── monorepo/
│
└── .github/workflows/
    ├── ci.yml                         # Current CI
    ├── release.yml                    # Signed releases
    └── fuzz.yml                       # Daily fuzz
```

## Common Commands

```bash
cargo check --workspace          # Kiểm tra compile
cargo test --workspace           # Chạy tests
cargo clippy --workspace         # Lint
cargo bench -p mgpm-bench       # Benchmark
cargo doc --open -p mgpm-cli    # Documentation
cargo run -- --help              # CLI help
cargo run -- audit              # Chạy audit
cargo run -- verify --deep       # Deep verify
```
