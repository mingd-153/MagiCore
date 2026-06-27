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
**Current task**: T0.1 — SQLite store index
**Branch hiện tại**: `week-1` (đã merge `development` vào đây)
**Branch gốc**: `development`
**Tests**: 217 passed, 0 failed, 0 warnings

## What's Been Done (Security Sprint)

| # | Feature | Files | Status |
|---|---------|-------|--------|
| 1 | Command injection fix | `main.rs` (run_script, exec_command) | ✅ |
| 2 | Advisory DB + GitHub fetch | `advisory_db.rs` | ✅ |
| 3 | Deep integrity verify | `main.rs` (cmd_verify_deep) | ✅ |
| 4 | Auth hardening | `auth.rs` | ✅ |
| 5 | Fuzz CI | `.github/workflows/fuzz.yml` | ✅ |
| 6 | Signed releases | `.github/workflows/release.yml` | ✅ |
| 7 | SECURITY.md | `SECURITY.md` (root) | ✅ |
| 8 | Sandbox module (stub) | `sandbox/` | ⚠️ Stub |
| 9 | TUF framework (stub) | `tuf.rs` | ⚠️ Stub |
| 10 | Dependency confusion check | `solver/mod.rs` | ✅ |
| 11 | `mgpm config` command | `main.rs` | ✅ |

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

## Next Steps (Tuần 1)

Task cho `week-1`:
1. **T0.1**: SQLite store index (`feat-T0.1-sqlite-store`) ← **bắt đầu ngay**
2. **T0.2**: CAS content-addressed import/export (`feat-T0.2-cas-io`)
3. **T0.4**: Lockfile integrity fix - BLAKE3 + real SHA-256 (`feat-T0.4-lockfile-integrity`)

## How to Continue

```bash
# 1. Đọc task chi tiết
cat tasks/phase-0-foundation/T0.1-sqlite-store.md

# 2. Tạo branch task từ week-1
git checkout week-1
git checkout -b feat-T0.1-sqlite-store

# 3. Implement theo vòng lặp: create → check → run → fix → update → fix → done → report → push
cargo check --workspace
cargo test --workspace
cargo clippy --workspace

# 4. Commit + merge vào week-1
git add -A
git commit -m "feat(mgpm): T0.1 - SQLite store index"
git checkout week-1
git merge feat-T0.1-sqlite-store

# 5. Xoá task branch (giữ week-1 sạch)
git branch -d feat-T0.1-sqlite-store

# 6. Update AGENTS.md
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
