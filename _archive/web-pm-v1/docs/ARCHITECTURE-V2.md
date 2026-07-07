# MGPM Architecture v2 — Kế Hoạch Tổng Thể

> Phiên bản: v2.0-draft | Ngày: 27/06/2026
> Mục tiêu: Package manager thông minh, nhanh, nhẹ, bảo mật, đa nền tảng

---

## Mục Lục

1. [Tầm Nhìn](#1-tầm-nhìn)
2. [So Sánh Chiến Lược Với Các Đối Thủ](#2-so-sánh-chiến-lược)
3. [Kiến Trúc Tổng Thể](#3-kiến-trúc-tổng-thể)
4. [Smart Store — Content-Addressable Package Store](#4-smart-store)
5. [Scaffolding Engine — Hệ Thống Tạo Dự Án](#5-scaffolding-engine)
6. [Monorepo Management](#6-monorepo-management)
7. [Security Architecture](#7-security-architecture)
8. [Performance Targets](#8-performance-targets)
9. [Cấu Trúc Thư Mục](#9-cấu-trúc-thư-mục)
10. [Lộ Trình Phát Triển (Roadmap)](#10-lộ-trình-phát-triển)
11. [Phase Chi Tiết](#11-phase-chi-tiết)

---

## 1. Tầm Nhìn

**MGPM** không chỉ là một package manager. Nó là một **hệ sinh thái quản lý dự án toàn diện** cho JavaScript/TypeScript:

```
MGPM = Package Manager + Project Scaffolder + Monorepo Orchestrator + Security Guardian
```

### Nguyên Tắc Thiết Kế

| # | Nguyên tắc | Mô tả |
|---|------------|-------|
| 1 | **Content-Addressable Store** | Mỗi file lưu 1 lần theo hash — tiết kiệm disk tối đa |
| 2 | **Strict Isolation** | Mặc định: không phantom dependencies, mỗi package chỉ thấy deps nó khai báo |
| 3 | **Deny-by-Default Security** | Scripts không chạy trừ khi được allow — như Bun, pnpm v10+, Yarn v4.14+ |
| 4 | **Zero-Copy I/O** | Hard links, CoW clone, memory-mapped cache |
| 5 | **Modular Scaffolding** | Plugin-based template system — extensible cho mọi framework |
| 6 | **Rust Native** | An toàn bộ nhớ, không GC, biên dịch tĩnh |
| 7 | **SQLite Store Index** | Như pnpm v11 — WAL mode, bundled manifests |
| 8 | **Supply Chain Defense** | Min release age, lockfile verify, SLSA provenance |

---

## 2. So Sánh Chiến Lược

### 2.1 Feature Coverage Target

| Domain | MGPM v1 | MGPM v2 (target) | pnpm v11 | Bun v1.3 | npm v11 | Yarn v4 |
|--------|:-------:|:-----------------:|:--------:|:--------:|:-------:|:-------:|
| **Store** | Basic CAS | SQLite CAS + GVS | ✅ SQLite | ✅ Dir cache | ✅ Cacache | ✅ Zip PnP |
| **Isolation** | Hoisted | Strict + symlink | ✅ Strict | ✅ Isolated | ❌ | ✅ PnP |
| **Speed** | ~1s warm | **<100ms warm** | ~400ms | ~300ms | ~5s | ~300ms |
| **Disk** | ~500 MB | **~150 MB** (10 proj) | ~1.8 GB | ~370 MB | ~8.4 GB | ~2.1 GB |
| **Scaffold** | ❌ | **`mg create-*`** | ❌ | `bun create` | `npm init` | ❌ |
| **Monorepo** | Basic | Filter + catalog + task | ✅ Full | Basic | Basic | Constraints |
| **Security** | Partial | **Defense-in-depth** | ✅ Strong | ✅ Strong | ✅ Strong | ✅ Strong |
| **Memory** | ~50MB RSS | **<30MB RSS idle** | ~80MB | ~40MB | ~120MB | ~60MB |

### 2.2 Disk Usage Projection (10 projects, average 100 deps each)

```
npm:   10 × 580 MB + 310 MB cache = 6,110 MB  ████████████████████████████████
Yarn:  10 × 520 MB + 280 MB cache = 5,480 MB   ██████████████████████████████
pnpm:  10 × 150 MB + 300 MB store = 1,800 MB   ████████
Bun:   10 × 120 MB + 250 MB cache = 1,450 MB   ███████
MGPM:  10 × 50 MB + 200 MB store  = 700 MB     ███        ← target
```

### 2.3 Warm Install Speed Target (50 deps, cached)

```
npm:   5,100 ms   ████████████████████████████████
pnpm:   394 ms    ██
Bun:    124 ms    █
MGPM:   100 ms    █                              ← target
```

---

## 3. Kiến Trúc Tổng Thể

### 3.1 Workspace Layout (Rust Crates)

```
web/mgpm/
├── Cargo.toml                          # workspace root
├── rust-toolchain.toml
├── crates/
│   ├── mgpm-core/                      # Core types, config, errors
│   ├── mgpm-store/                     # Content-addressed store (SQLite v2)
│   ├── mgpm-registry/                  # Registry clients (npm, JSR, git, custom)
│   ├── mgpm-resolver/                  # PubGrub resolver + supply chain checks
│   ├── mgpm-lockfile/                  # Lockfile (binary + text format)
│   ├── mgpm-installer/                 # Package installation pipeline
│   ├── mgpm-linker/                    # node_modules linking strategy
│   ├── mgpm-sandbox/                   # Process sandbox (macOS/Linux/Windows)
│   ├── mgpm-scaffold/                  # 🆕 Scaffolding engine
│   ├── mgpm-monorepo/                  # 🆕 Monorepo orchestration
│   ├── mgpm-cache/                     # 🆕 Cache layer (memory + disk + remote)
│   ├── mgpm-security/                  # 🆕 Advisory DB, TUF, Sigstore, SBOM
│   ├── mgpm-script/                    # 🆕 Lifecycle script manager
│   ├── mgpm-daemon/                    # 🆕 Watch daemon + GC
│   ├── mgpm-cli/                       # CLI (thin layer over crates)
│   └── mgpm-bench/                     # Benchmarks
├── fuzz/                               # Fuzz targets
│   ├── Cargo.toml
│   └── targets/
├── docs/
│   ├── ARCHITECTURE-V2.md
│   └── SECURITY-REPORT.md
├── templates/                          # 🆕 Scaffolding templates
│   ├── vanilla/                        # JS/TS + HTML + CSS
│   ├── react/                          # React SPA
│   ├── next/                           # Next.js fullstack
│   ├── vue/                            # Vue 3
│   ├── express/                        # Express backend
│   ├── fastify/                        # Fastify backend
│   └── monorepo/                       # Monorepo workspace
├── install.sh
└── mgpm.asc
```

### 3.2 Data Flow

```
User Command
     │
     ▼
  mgpm-cli (arg parsing, routing)
     │
     ├──> mgpm-scaffold → tạo project mới, sinh file, cấu hình
     │
     ├──> mgpm-installer
     │       ├── mgpm-resolver (PubGrub)
     │       │     └── mgpm-security (advisory, dep confusion)
     │       ├── mgpm-registry (download)
     │       │     └── mgpm-cache (memory + disk cache)
     │       ├── mgpm-store (CAS import)
     │       ├── mgpm-linker (symlink vào node_modules)
     │       ├── mgpm-script (lifecycle scripts)
     │       └── mgpm-sandbox (process isolation)
     │
     ├──> mgpm-monorepo
     │       ├── Task graph (build/test/lint ordering)
     │       ├── Filter engine (--filter syntax)
     │       └── Catalogs (shared versions)
     │
     ├──> mgpm-daemon
     │       ├── Watch filesystem
     │       ├── GC store
     │       └── Auto-update advisory DB
     │
     └──> mgpm-security
             ├── Audit
             ├── TUF update
             ├── Sigstore verify
             └── SBOM export
```

---

## 4. Smart Store

### 4.1 Store Architecture (SQLite v2)

Lấy cảm hứng từ pnpm v11 store nhưng cải tiến:

```
~/.mgpm/store/v2/
├── index.db                    # SQLite index (WAL mode)
├── CAS/                        # Content-addressed files
│   ├── 00/
│   │   ├── a1b2c3d4...        # SHA-256 hash → filename
│   │   └── ...
│   ├── 01/
│   └── ... (256 shards)
├── manifests/                  # Bundled package.json metadata
│   ├── lodash/
│   │   └── 4.17.21.json       # Pre-parsed manifest
│   └── ...
├── patches/                    # Patched packages (like pnpm)
│   └── express@4.18.2.patch
├── links/                      # Global Virtual Store (GVS)
│   └── <dep-graph-hash>/
│       └── node_modules/
└── projects/                   # Project registry for GC
    └── <project-path-hash> → symlink to project root
```

### 4.2 SQLite Schema

```sql
-- Store index.db schema
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 536870912;  -- 512 MB
PRAGMA cache_size = -32000;    -- 32 MB
PRAGMA temp_store = MEMORY;
PRAGMA wal_autocheckpoint = 10000;

CREATE TABLE packages (
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    integrity TEXT NOT NULL,        -- sha256-<base64>
    shard TEXT NOT NULL,            -- 2-char prefix
    filename TEXT NOT NULL,         -- full hash
    is_executable BOOLEAN DEFAULT 0,
    manifest_json TEXT,             -- bundled package.json
    size_bytes INTEGER,
    compressed_size_bytes INTEGER,
    created_at INTEGER DEFAULT (unixepoch()),
    PRIMARY KEY (integrity)
) WITHOUT ROWID;

CREATE INDEX idx_packages_name ON packages(name, version);

CREATE TABLE projects (
    project_hash TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    last_used INTEGER DEFAULT (unixepoch())
);

CREATE TABLE integrity_cache (
    file_path TEXT PRIMARY KEY,
    integrity TEXT NOT NULL,
    mtime INTEGER NOT NULL
);
```

### 4.3 Store Operations

| Operation | Implementation | Target latency |
|-----------|---------------|----------------|
| **Import** | Compute SHA-256 → check index → hard link to CAS | < 1ms/file |
| **Export** | Read from CAS → hard link to node_modules | < 0.5ms/file |
| **Verify** | Re-hash file → compare with index | < 2ms/file |
| **GC** | Mark-and-sweep via project registry | < 500ms |
| **Prune** | Remove unreferenced CAS files | < 1s |
| **Status** | Verify all files in index | < 2s (1000 files) |

### 4.4 Global Virtual Store (GVS)

Giống pnpm v11 + Bun global store:

- Khi `mgpm install --linker isolated` (mặc định trong workspace):
  - Dependencies được link từ store vào `node_modules/.mgpm/<hash>/`
  - Mỗi project chỉ có symlinks (~5 MB)
  - 10 projects dùng chung store = ~200 MB (store) + 50 MB (symlinks) = **~250 MB total**

### 4.5 Cache Layer

```
Cache Layer (mgpm-cache)
├── Memory Cache (L1)
│   ├── Package manifests (LRU, 1000 entries)
│   ├── Resolution results (LRU, 500 entries)
│   └── Store index hot data
├── Disk Cache (L2)
│   ├── Registry responses (ETag-based)
│   ├── Binary manifest cache (.mgpm files)
│   └── Tarball cache
└── Remote Cache (L3)  [future]
    ├── Share cache across CI
    └── S3/Azure/GCS backend
```

### 4.6 Memory Optimizations (Lấy cảm hứng từ Bun)

| Kỹ thuật | Mô tả | Nguồn cảm hứng |
|-----------|-------|----------------|
| **mimalloc allocator** | Thay thế glibc allocator | Bun (Zig) |
| **Memory-mapped cache** | Đọc file `.mgpm` cache bằng mmap | Bun `.npm` files |
| **Arena allocation** | Nhóm allocation cho parser, giải phóng bulk | Bun `NewStore.zig` |
| **SoA data layout** | Package manifest lưu theo cột, không object | Bun SoA lockfile |
| **Gzip trailer pre-allocation** | Đọc 4 byte cuối của gzip để biết kích thước uncompressed | Bun tarball extraction |
| **Zero-copy JSON parsing** | String slices, không allocation | Bun JSON parser |
| **Pre-computed resolution** | Lockfile lưu resolved package IDs, không ranges | Bun resolution |

### 4.7 Target Benchmarks

| Scenario | MGPM v1 | MGPM v2 target | pnpm v11 | Bun v1.3 |
|----------|:-------:|:---------------:|:--------:|:--------:|
| Cold install (50 deps) | ~15s | **<3s** | ~15.7s | ~0.8s |
| Warm install (50 deps) | ~5s | **<100ms** | ~394ms | ~124ms |
| Lockfile parse (100 pkgs) | ~33µs | **<10µs** | ~50µs | ~45µs |
| Resolution (100 pkgs) | ~280µs | **<50µs** | ~150µs | ~100µs |
| Store import (100KB) | ~1ms | **<200µs** | ~500µs | ~300µs |
| RSS memory idle | ~50MB | **<30MB** | ~80MB | ~40MB |
| Binary size | ~15MB | **<8MB** (stripped) | ~20MB | ~15MB |

---

## 5. Scaffolding Engine

### 5.1 `mg create-*` Command System

```
mg create <type> [name] [options]

Types:
  mg create web          # JS/TS + HTML + CSS (vanilla)
  mg create react        # React SPA (Vite-based)
  mg create next         # Next.js fullstack
  mg create vue          # Vue 3 + Vite
  mg create express      # Express backend
  mg create fastify      # Fastify backend
  mg create node-lib     # Node.js library
  mg create cli          # CLI tool (with clap-like args)
  mg create monorepo     # Monorepo workspace
  mg create template     # Custom template from GitHub/npm/local
```

### 5.2 Scaffolding Architecture

```
mgpm-scaffold/
├── src/
│   ├── lib.rs                    # Public API
│   ├── engine/
│   │   ├── mod.rs                # ScaffoldEngine trait
│   │   ├── static.rs             # Static template copier (như create-vite)
│   │   ├── modular.rs            # Modular installer (như create-t3-app)
│   │   └── generator.rs          # File generator với variables
│   ├── templates/                # Bundled templates
│   │   ├── vanilla/
│   │   ├── react/
│   │   ├── next/
│   │   ├── vue/
│   │   ├── express/
│   │   ├── fastify/
│   │   ├── node-lib/
│   │   ├── cli/
│   │   └── monorepo/
│   ├── installers/               # Modular installers
│   │   ├── mod.rs
│   │   ├── typescript.rs         # Adds tsconfig.json
│   │   ├── tailwind.rs           # Adds tailwind.config
│   │   ├── eslint.rs             # Adds eslint config
│   │   ├── prettier.rs           # Adds prettier config
│   │   ├── vitest.rs             # Adds vitest config
│   │   ├── docker.rs             # Adds Dockerfile
│   │   ├── ci.rs                 # Adds GitHub Actions
│   │   └── git.rs                # Adds .gitignore
│   ├── prompts/                  # Interactive prompts
│   │   ├── mod.rs
│   │   ├── react.rs
│   │   ├── next.rs
│   │   └── monorepo.rs
│   └── post/                     # Post-create hooks
│       ├── mod.rs
│       ├── git_init.rs
│       ├── install_deps.rs
│       └── success_msg.rs
```

### 5.3 Template System

**Static templates** (cho tốc độ — như `create-vite`):
```
templates/next/
├── package.json.hbs          # Handlebars template
├── tsconfig.json.hbs
├── next.config.ts.hbs
├── tailwind.config.ts.hbs
├── postcss.config.mjs.hbs
├── eslint.config.mjs.hbs
├── src/
│   ├── app/
│   │   ├── layout.tsx.hbs
│   │   ├── page.tsx.hbs
│   │   └── globals.css.hbs
│   ├── components/
│   │   └── ui/
│   │       └── button.tsx.hbs
│   └── lib/
│       └── utils.ts.hbs
├── public/
│   └── favicon.ico
└── .gitignore.hbs
```

**Modular installers** (cho linh hoạt — như `create-t3-app`):
```rust
// Installer trait
#[async_trait]
pub trait Installer {
    fn name(&self) -> &str;
    fn dependencies(&self) -> Vec<String>;
    async fn install(&self, ctx: &ScaffoldContext) -> Result<(), Error>;
}

// Ví dụ: Tailwind installer
pub struct TailwindInstaller;

#[async_trait]
impl Installer for TailwindInstaller {
    fn name(&self) -> &str { "tailwind" }
    fn dependencies(&self) -> Vec<String> {
        vec!["tailwindcss".into(), "postcss".into(), "autoprefixer".into()]
    }
    async fn install(&self, ctx: &ScaffoldContext) -> Result<(), Error> {
        if ctx.framework == "next" {
            ctx.write_file("tailwind.config.ts", include_str!("templates/tailwind/next.config.ts"))?;
        }
        ctx.write_file("postcss.config.mjs", include_str!("templates/tailwind/postcss.config.mjs"))?;
        ctx.append_file("src/app/globals.css", "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n")?;
        Ok(())
    }
}
```

### 5.4 Generated Folder Structures

#### `mg create web my-app` — Vanilla JS/TS

```
my-app/
├── src/
│   ├── index.html
│   ├── main.ts              # Entry point
│   ├── styles/
│   │   └── main.css
│   ├── components/          # Web components
│   │   └── app.ts
│   └── utils/
│       └── helpers.ts
├── public/
│   └── favicon.ico
├── package.json
├── tsconfig.json
├── vite.config.ts
├── .gitignore
├── .env.example
├── .editorconfig
└── README.md
```

#### `mg create react my-app` — React SPA

```
my-app/
├── src/
│   ├── main.tsx             # Entry
│   ├── App.tsx
│   ├── pages/
│   │   ├── home.tsx
│   │   └── about.tsx
│   ├── components/
│   │   ├── ui/              # Base UI primitives
│   │   │   ├── button.tsx
│   │   │   ├── input.tsx
│   │   │   └── card.tsx
│   │   └── features/        # Feature components
│   │       └── header.tsx
│   ├── hooks/               # Custom hooks
│   │   └── use-auth.ts
│   ├── services/            # API calls
│   │   └── api.ts
│   ├── stores/              # State (zustand)
│   │   └── auth-store.ts
│   ├── types/               # TypeScript types
│   │   └── index.ts
│   └── utils/
│       └── helpers.ts
├── public/
│   └── favicon.ico
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
├── eslint.config.mjs
├── .gitignore
├── .env.example
└── README.md
```

#### `mg create next my-app` — Next.js Fullstack

```
my-app/
├── src/
│   ├── app/
│   │   ├── (marketing)/      # Route groups
│   │   │   └── page.tsx
│   │   ├── (dashboard)/
│   │   │   ├── layout.tsx
│   │   │   └── page.tsx
│   │   ├── api/
│   │   │   └── route.ts
│   │   ├── layout.tsx        # Root layout
│   │   └── page.tsx          # Home
│   ├── components/
│   │   ├── ui/
│   │   └── features/
│   ├── lib/
│   │   ├── utils.ts
│   │   └── db.ts             # Database client
│   ├── actions/              # Server Actions
│   │   └── auth.ts
│   └── styles/
│       └── globals.css
├── public/
│   └── favicon.ico
├── package.json
├── tsconfig.json
├── next.config.ts
├── tailwind.config.ts
├── postcss.config.mjs
├── eslint.config.mjs
├── .env.example
├── .env.local.example
├── Dockerfile
├── .github/workflows/ci.yml
├── .gitignore
└── README.md
```

#### `mg create express my-api` — Express Backend

```
my-api/
├── src/
│   ├── index.ts              # Entry
│   ├── app.ts                # Express setup
│   ├── routes/
│   │   ├── index.ts
│   │   └── v1/
│   │       ├── users.ts
│   │       └── auth.ts
│   ├── controllers/
│   │   ├── user-controller.ts
│   │   └── auth-controller.ts
│   ├── services/             # Business logic
│   │   ├── user-service.ts
│   │   └── auth-service.ts
│   ├── middleware/
│   │   ├── auth.ts
│   │   ├── validate.ts
│   │   ├── rate-limit.ts
│   │   └── error-handler.ts
│   ├── models/               # Prisma/Drizzle schemas
│   │   └── user.ts
│   ├── validators/           # Zod schemas
│   │   └── user-schema.ts
│   ├── types/
│   │   └── index.ts
│   └── config/
│       ├── env.ts            # Env validation (Zod)
│       └── app.ts
├── prisma/
│   └── schema.prisma
├── tests/
│   ├── unit/
│   └── integration/
├── Dockerfile
├── docker-compose.yml
├── .github/workflows/ci.yml
├── package.json
├── tsconfig.json
├── eslint.config.mjs
├── .env.example
├── .gitignore
└── README.md
```

#### `mg create monorepo my-project` — Monorepo Workspace

```
my-project/
├── apps/
│   ├── web/                  # Next.js frontend
│   │   ├── src/
│   │   │   ├── app/
│   │   │   ├── components/
│   │   │   └── lib/
│   │   ├── package.json      # dependencies: {"ui": "workspace:*"}
│   │   └── tsconfig.json
│   └── api/                  # Express backend
│       ├── src/
│       │   ├── routes/
│       │   ├── services/
│       │   └── middleware/
│       ├── package.json      # dependencies: {"shared": "workspace:*"}
│       └── tsconfig.json
├── packages/
│   ├── ui/                   # Shared React components
│   │   ├── src/
│   │   │   ├── button.tsx
│   │   │   ├── card.tsx
│   │   │   └── index.ts
│   │   ├── package.json
│   │   └── tsconfig.json
│   ├── shared/               # Shared utilities
│   │   ├── src/
│   │   │   ├── utils.ts
│   │   │   └── types.ts
│   │   ├── package.json
│   │   └── tsconfig.json
│   └── config/               # Shared configs
│       ├── eslint/
│       ├── tsconfig/
│       └── prettier/
├── mgpm.yaml                 # MGPM workspace config
├── mgpm.lock
├── turbo.json                # Task orchestration
├── package.json              # Root workspace
├── tsconfig.base.json
├── .github/workflows/
│   └── ci.yml
├── .gitignore
└── README.md
```

### 5.5 Interactive Prompts

```
$ mg create web my-app
✔ Project name: my-app
✔ TypeScript? › Yes / No
✔ Package manager: › mgpm / npm / pnpm / yarn / bun
? Which features:
  ◻ Tailwind CSS
  ◻ ESLint
  ◻ Prettier
  ◻ Vitest
  ◻ Docker
  ◻ CI (GitHub Actions)
  ◻ Husky + lint-staged
✔ Git init? › Yes / No
✔ Install dependencies? › Yes / No
```

### 5.6 `mg create template` — Custom Templates

```bash
# Từ GitHub repo
mg create template my-app --from https://github.com/user/next-starter

# Từ npm package
mg create template my-app --from create-t3-app

# Từ local path
mg create template my-app --from ./my-custom-template

# Với template variables
mg create template my-app --from ./template --var name=my-app --var author="Me"
```

---

## 6. Monorepo Management

### 6.1 Workspace Config (`mgpm.yaml`)

```yaml
# mgpm.yaml — root workspace configuration
workspace:
  packages:
    - "apps/*"
    - "packages/*"
    - "!packages/deprecated"
  catalog:
    react: "^19.0.0"
    typescript: "~5.7.0"
    vitest: "^3.0.0"
  catalogs:
    react18:
      react: "^18.3.0"

linker: isolated           # hoisted | isolated | pnp
shared-lockfile: true
hoist: false               # Don't hoist to root node_modules

scripts:
  build:
    depends-on: ["^build"]
    cache: true
    inputs: ["src/**/*.ts", "src/**/*.tsx"]
    outputs: ["dist/**"]
  test:
    depends-on: ["build"]
    cache: true
  lint:
    cache: true
  dev:
    cache: false
    persistent: true

security:
  min-release-age: 24h     # Chặn packages dưới 24h tuổi
  block-exotic-deps: true  # Chặn git/URL deps
  trusted-registries:
    - "https://registry.npmjs.org"
    - "https://npm.pkg.github.com"
```

### 6.2 Filter Syntax (Lấy cảm hứng từ pnpm --filter)

```
mg [command] --filter=<selector>

Selectors:
  --filter=@scope/pkg         Package name
  --filter=@scope/*           Glob
  --filter=pkg...             Package + dependencies
  --filter=...pkg             Package + dependents
  --filter=...pkg...          Package + deps + dependents
  --filter="{apps/*}[main]"   Changed packages in directory vs main
  --filter="[HEAD~1]"         Changed since last commit

Examples:
  mg build --filter=@myorg/ui...
  mg test --filter=...@myorg/web
  mg lint --filter="{packages/*}[main]"
  mg run build --filter=@myorg/app --parallel
```

### 6.3 Task Orchestration (Lấy cảm hứng từ Turborepo + Nx)

```
Task Pipeline:
  1. Parse workspace config (mgpm.yaml)
  2. Build package graph (from package.json workspace deps)
  3. Build task graph (depends-on resolution)
  4. Topological sort
  5. Execute in parallel (respecting concurrency limits)
  6. Cache outputs (local + optional remote)

Cache key = hash(
    source_files(inputs),
    task_config,
    env_vars,
    dependency_task_hashes,
    lockfile_hash
)
```

### 6.4 Catalogs (Lấy cảm hứng từ pnpm catalogs)

```yaml
# mgpm.yaml
catalog:
  react: "^19.0.0"
  next: "^15.0.0"
  typescript: "~5.7.0"
```

```json
// apps/web/package.json
{
  "dependencies": {
    "react": "catalog:",
    "next": "catalog:"
  },
  "devDependencies": {
    "typescript": "catalog:"
  }
}
```

### 6.5 Workspace Protocol

```json
// packages/shared/package.json
{
  "name": "@myorg/shared",
  "version": "1.0.0"
}

// apps/web/package.json
{
  "dependencies": {
    "@myorg/shared": "workspace:*",
    "@myorg/utils": "workspace:^2.0.0"
  }
}
```

Trên `mgpm publish`, `workspace:*` được thay bằng version thật.

---

## 7. Security Architecture

### 7.1 Defense-in-Depth Layers

```
Layer 1: Supply Chain Prevention
├── Min release age (24h default)
├── Block exotic deps (git/URL tarballs)
├── Trusted registries allowlist
└── Dependency confusion checks

Layer 2: Integrity Verification
├── Content-addressed store (SHA-256)
├── Lockfile integrity (BLAKE3)
├── SRI verification on install
└── mgpm verify --deep (node_modules walk)

Layer 3: Lifecycle Script Security
├── Deny-by-default (scripts blocked)
├── trustedDependencies allowlist
├── --ignore-scripts flag
└── Interactive approve-builds

Layer 4: Runtime Protection
├── Process sandbox (--sandbox flag)
├── Token leak detection
├── Auth hardening
└── Redact secrets in logs

Layer 5: Release Security
├── GPG signing
├── Sigstore/cosign keyless
├── SLSA provenance
└── SBOM generation (CycloneDX + SPDX)
```

### 7.2 Security Defaults

| Setting | MGPM v1 | MGPM v2 default | Lý do |
|---------|:-------:|:----------------:|-------|
| Script execution | Allowed | **Denied** | Giống Bun, pnpm v10+, Yarn v4.14+ |
| Lockfile frozen | ❌ | **✅ CI mode** | Ngăn lockfile drift |
| Min release age | ❌ | **24h** | Chống malicious publish |
| Block exotic deps | ❌ | **✅** | Chống git/URL tarball độc hại |
| Audit on install | ❌ | **✅** | Check vulns mỗi lần cài |
| Integrity verify | ❌ | **✅** | Verify mọi byte |
| Store verify | ❌ | **✅** | Verify store integrity |

### 7.3 TUF Integration (Đầy đủ, không stub)

```rust
// mgpm-security/src/tuf.rs — implementation plan
pub struct TufClient {
    root_keys: Vec<ed25519::PublicKey>,
    threshold: u8,
    repo_url: String,
}

impl TufClient {
    /// Verify root metadata signature
    fn verify_root(&self, metadata: &RootMetadata) -> Result<()>;
    
    /// Verify targets metadata (signed by snapshot key)
    fn verify_targets(&self, metadata: &TargetsMetadata) -> Result<()>;
    
    /// Download and verify advisory update
    async fn update(&self) -> Result<Vec<Advisory>>;
    
    /// Check cache freshness (24h TTL)
    fn is_cache_fresh(&self) -> bool;
}
```

### 7.4 Supply Chain Gates

```
[Publish] → [MinReleaseAge] → [BlockExoticDeps] → [TrustedRegistries] → [Install]

[Resolution] → [DependencyConfusionCheck] → [AdvisoryCheck] → [IntegrityVerify]
```

---

## 8. Performance Targets

### 8.1 Hard Goals

| Metric | Current (v1) | Target (v2) | Compared to |
|--------|:------------:|:-----------:|:-----------:|
| Warm install (50 deps) | ~5s | **<100ms** | Bun ~124ms |
| Cold install (50 deps) | ~15s | **<3s** | pnpm ~15.7s |
| Resolution (100 pkgs) | ~280µs | **<50µs** | Bun ~100µs |
| Lockfile parse (100 pkgs) | ~33µs | **<10µs** | Bun ~45µs |
| Lockfile serialize (100 pkgs) | ~93µs | **<30µs** | Bun ~40µs |
| Store import (1KB) | ~611µs | **<100µs** | pnpm ~300µs |
| Store import (100KB) | ~980µs | **<200µs** | pnpm ~500µs |
| Binary size (stripped) | ~15MB | **<8MB** | Bun ~15MB |
| RSS memory (idle) | ~50MB | **<30MB** | Bun ~40MB |
| Disk (10 proj) | ~500MB | **~250MB** | pnpm ~1.8GB |

### 8.2 Key Optimization Strategies

1. **SQLite store index** (giống pnpm v11): WAL mode, mmap, bundled manifests
2. **mimalloc allocator**: Thay thế system allocator, giảm memory fragmentation
3. **Memory-mapped cache**: Đọc cache bằng mmap — zero copy
4. **Gzip trailer pre-allocation**: Đọc uncompressed size từ 4 byte cuối gzip
5. **Arena allocation**: Bulk allocation cho parser — free cả arena khi xong
6. **SoA data layout**: Package metadata lưu theo column, cache-friendly
7. **Pre-computed resolution**: Lockfile lưu resolved package IDs
8. **Concurrent downloads**: io_uring (Linux) / kqueue (macOS) — 48 connections
9. **Zero-copy JSON**: String slices từ buffer, không allocation cho field names
10. **ETag cache validation**: 304 Not Modified = 0 byte transfer

### 8.3 Memory Budget

```
Component               Target RSS
──────────────────────────────────
mgpm-cli binary           8 MB     (stripped)
Store index (SQLite)      5 MB     (mmap 32 MB)
Cache (memory)            5 MB     (LRU 1000 entries)
Resolution state          2 MB     (arena allocated)
HTTP connections          3 MB     (48 connections buffer)
Tarball buffers           5 MB     (pre-allocated)
Other                     2 MB
──────────────────────────────────
Total                     30 MB
```

---

## 9. Cấu Trúc Thư Mục

### 9.1 User Home (`~/.mgpm/`)

```
~/.mgpm/
├── store/
│   └── v2/
│       ├── index.db           # SQLite index
│       ├── CAS/               # Content-addressed files
│       ├── manifests/         # Bundled manifests
│       ├── patches/           # Package patches
│       ├── links/             # Global virtual store
│       └── projects/          # Project registry
├── cache/
│   ├── metadata/              # Registry responses
│   ├── tarballs/              # Downloaded tarballs
│   └── resolutions/           # Resolution cache
├── security/
│   ├── advisories.json        # Cached advisory DB
│   ├── tuf/                   # TUF metadata
│   └── keys/                  # GPG + Sigstore keys
├── config.toml                # User config
└── daemon.pid                 # Daemon PID file
```

### 9.2 Project Root

```
my-project/
├── src/
│   └── ...                    # Source code
├── node_modules/
│   ├── .mgpm/                 # Virtual store (symlinks)
│   │   └── <hash>/
│   │       └── node_modules/
│   ├── .bin/                  # Binary symlinks
│   ├── <direct-deps>/         # Symlinks → .mgpm/...
│   └── ...
├── .mgpm/                     # MGPM project data
│   ├── lock/                  # Lockfile cache
│   └── state/                 # Installation state
├── mgpm.lock                  # Lockfile (bincode + TOML)
├── mgpm.yaml                  # Project config
├── package.json
└── ...
```

### 9.3 Rust Crate Responsibilities

| Crate | Responsibility | Dependencies | Size (LOC) |
|-------|---------------|--------------|:----------:|
| `mgpm-core` | Types, config, errors, logging | — | ~2K |
| `mgpm-store` | SQLite CAS store, import/export | mgpm-core, rusqlite, sha2 | ~3K |
| `mgpm-registry` | Registry clients, HTTP, auth | mgpm-core, reqwest | ~2K |
| `mgpm-resolver` | PubGrub resolution, dep graph | mgpm-core, mgpm-store | ~3K |
| `mgpm-lockfile` | Lockfile format (TOML + bincode) | mgpm-core, toml, bincode | ~2K |
| `mgpm-installer` | Install pipeline orchestration | mgpm-core, mgpm-store, mgpm-registry, mgpm-resolver, mgpm-linker, mgpm-script, mgpm-sandbox | ~2K |
| `mgpm-linker` | node_modules linking strategies | mgpm-core, mgpm-store | ~2K |
| `mgpm-script` | Lifecycle script management | mgpm-core | ~1K |
| `mgpm-sandbox` | Process sandbox (macOS/Linux/Windows) | mgpm-core | ~1K |
| `mgpm-scaffold` | 🆕 Scaffolding engine | mgpm-core, handlebars | ~5K |
| `mgpm-monorepo` | 🆕 Task graph, filter, catalogs | mgpm-core, mgpm-resolver | ~4K |
| `mgpm-cache` | 🆕 Cache layer (mem + disk + remote) | mgpm-core, lru, mmap | ~2K |
| `mgpm-security` | 🆕 Advisory, TUF, Sigstore, SBOM | mgpm-core, tough, sigstore-rs | ~3K |
| `mgpm-daemon` | 🆕 Watch daemon, GC, auto-update | mgpm-core, mgpm-store, mgpm-security | ~2K |
| `mgpm-cli` | CLI (clap, thin routing) | Tất cả crates trên | ~3K |
| **Total** | | | **~40K** |

---

## 10. Lộ Trình Phát Triển (Roadmap)

```
Phase 0: Nền Tảng (Weeks 1-4) — ĐANG LÀM
├── SQLite store index + CAS
├── Content-addressable import/export
├── Lockfile integrity hash (SHA-256)
├── Fix resolver integrity hash (từ tên → từ content)
└── GVS (Global Virtual Store)

Phase 1: Tốc Độ (Weeks 5-8)
├── mimalloc allocator
├── Memory-mapped cache
├── Arena allocation
├── ETag cache validation
├── Concurrent downloads (io_uring/kqueue)
├── Zero-copy JSON parsing
└── Pre-computed lockfile resolution

Phase 2: Scaffolding (Weeks 9-14)
├── Scaffold engine (static + modular)
├── Templates: vanilla, react, next, vue
├── Templates: express, fastify, node-lib, cli
├── Interactive prompts (@clack-like)
├── Post-create hooks (git init, install)
└── `mg create` với custom templates

Phase 3: Monorepo (Weeks 15-20)
├── Workspace config (mgpm.yaml)
├── Filter engine (--filter)
├── Task graph + orchestration
├── Catalogs (shared versions)
├── Workspace protocol (workspace:*)
├── Cache (local + remote)
└── Affected commands

Phase 4: Security Hoàn Thiện (Weeks 21-26)
├── TUF verification (Ed25519)
├── Sigstore integration
├── SLSA provenance
├── SBOM (CycloneDX + SPDX)
├── Script security (deny-by-default)
├── Process sandbox (thực tế)
└── Supply chain gates

Phase 5: Optimization & Polish (Weeks 27-32)
├── Remote cache (S3/Azure/GCS)
├── Daemon mode + auto-GC
├── Performance tuning
├── Windows support
├── WASM plugin system
└── Benchmark suite

Phase 6: Ecosystem (Weeks 33-40)
├── Plugin registry
├── mgpm hub (template sharing)
├── VSCode extension
├── GitHub Actions integration
├── Dependabot support
└── Documentation site
```

---

## 11. Phase Chi Tiết

### Phase 0: Nền Tảng (Weeks 1-4)

**Mục tiêu:** Có store hoạt động, lockfile integrity thật, GVS.

| Week | Tasks | Deliverables |
|:----:|-------|-------------|
| 1 | SQLite store schema + index.db | `mgpm-store` với SQLite backend |
| 1 | CAS import/export (SHA-256) | Import file → CAS, export → hard link |
| 2 | Store verify + status | `mgpm store verify`, `mgpm store status` |
| 2 | Lockfile integrity fix | Thay SipHash → BLAKE3, thay hex(name) → SHA-256 |
| 3 | GVS (Global Virtual Store) | `node_modules/.mgpm/<hash>/` layout |
| 3 | `--linker isolated` flag | Strict symlink mode |
| 4 | Integration test + benchmark | So sánh với pnpm/Bun |

**Key files:**
- `crates/mgpm-store/src/store/sqlite.rs` — SQLite index
- `crates/mgpm-store/src/store/cas.rs` — Content-addressed storage
- `crates/mgpm-store/src/store/gvs.rs` — Global virtual store
- `crates/mgpm-store/src/lib.rs` — Public API

### Phase 1: Tốc Độ (Weeks 5-8)

**Mục tiêu:** Warm install < 100ms, cold install < 3s.

| Week | Tasks | Target Improvement |
|:----:|-------|:------------------:|
| 5 | mimalloc allocator | -20% memory, -10% time |
| 5 | Memory-mapped cache | -30% cache read time |
| 6 | Arena allocation cho parser | -40% parser allocs |
| 6 | ETag cache validation | -80% registry GETs |
| 7 | Concurrent downloads (io_uring) | -50% download time |
| 7 | Zero-copy JSON parsing | -30% manifest parse |
| 8 | Pre-computed lockfile resolution | -60% resolution time |
| 8 | Benchmark + tune | Target verification |

### Phase 2: Scaffolding (Weeks 9-14)

**Mục tiêu:** `mg create-*` hoàn chỉnh với 10+ templates.

| Week | Tasks | Templates |
|:----:|-------|:---------:|
| 9 | Scaffold engine design + Handlebars | — |
| 9 | Static template copier | — |
| 10 | Modular installer system | — |
| 10 | Interactive prompts | — |
| 11 | Template: vanilla JS/TS | ✅ vanilla |
| 11 | Template: React SPA | ✅ react |
| 12 | Template: Next.js | ✅ next |
| 12 | Template: Vue 3 | ✅ vue |
| 13 | Template: Express | ✅ express |
| 13 | Template: Fastify | ✅ fastify |
| 14 | Template: node-lib, cli, monorepo | ✅ 3 templates |
| 14 | `mg create template` custom | — |

### Phase 3: Monorepo (Weeks 15-20)

**Mục tiêu:** Workspace quản lý thông minh, filter, task graph.

| Week | Tasks |
|:----:|-------|
| 15 | Workspace config parser (mgpm.yaml) |
| 15 | Package graph builder |
| 16 | Filter engine (--filter syntax) |
| 16 | Workspace protocol (workspace:*) |
| 17 | Task graph + topological sort |
| 17 | Task execution (parallel) |
| 18 | Catalogs (shared version pinning) |
| 18 | Cache (local + remote) |
| 19 | Affected commands (git diff) |
| 19 | `mg run` trong workspace context |
| 20 | Integration tests + benchmarks |

### Phase 4: Security (Weeks 21-26)

**Mục tiêu:** Security hoàn chỉnh, không stub, defense-in-depth.

| Week | Tasks |
|:----:|-------|
| 21 | TUF: root key generation |
| 21 | TUF: Ed25519 signature verify |
| 22 | TUF: metadata download + verify |
| 22 | TUF: auto-update daemon |
| 23 | Sigstore: Fulcio OIDC flow |
| 23 | Sigstore: Rekor transparency log |
| 24 | SLSA: provenance attestation |
| 24 | SBOM: CycloneDX 1.7 format |
| 25 | SBOM: SPDX 2.3 format |
| 25 | Script deny-by-default + trustedDependencies |
| 26 | Process sandbox (real macOS/Linux) |
| 26 | Supply chain gates + integration test |

### Phase 5-6: (Xem roadmap ở trên)

---

## Phụ Lục A: So Sánh Đặc Tính Kỹ Thuật

### A.1 Store Engine

| Feature | pnpm v11 | Bun v1.3 | MGPM v2 |
|---------|:--------:|:--------:|:-------:|
| Store backend | SQLite | Dir cache | **SQLite** |
| Hash algorithm | SHA-512 | SHA-512 | **SHA-256** |
| Deduplication | Per-file hash | Per-package dir | **Per-file hash** |
| Cross-project | Global store | Cache + hardlink | **GVS + hardlink** |
| GC | mark-and-sweep | N/A | **mark-and-sweep** |
| Bundled manifests | ✅ v11 | ❌ | **✅** |
| Store verify | `pnpm store status` | N/A | **`mgpm store verify`** |
| Integrity check | On install | On install | **On install + verify --deep** |

### A.2 Install Speed Factors

| Factor | pnpm | Bun | MGPM v2 target |
|--------|:----:|:---:|:--------------:|
| HTTP parallelism | ~16 | 48 | **48** |
| I/O model | libuv (Rust) | io_uring (Zig) | **io_uring + kqueue** |
| JSON parsing | serde_json | Custom zero-copy | **Zero-copy + SoA** |
| Lockfile format | YAML | JSONC | **bincode** |
| Store linking | hardlink | clonefile/hardlink | **clonefile + hardlink** |
| Resolution cache | SQLite | In-memory | **SQLite + mem LRU** |

---

*Tài liệu này được xây dựng dựa trên research thực tế từ pnpm v11, Bun v1.3, npm v11, Yarn v4, Nx 22.7, Turborepo 2.x và mã nguồn MGPM hiện tại.*
