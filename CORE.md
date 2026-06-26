# MegaGate Core Documentation

## 1. Project Overview

**MegaGate** is a multi-platform package manager core + project scaffolding tool. Rust SSOT (Single Source of Truth), exposed via FFI/bindings to all major languages and platforms.

| Layer | Công nghệ | Vai trò |
|-------|-----------|---------|
| **Core engine** | Rust (`crates/`) | Resolver, fetcher, linker, security, lockfile |
| **CLI** | TypeScript (`cli/`) | User-facing CLI, gọi Rust core qua NAPI-RS |
| **Web app** | TypeScript/React (`web/`) | Web frontend SPA |
| **Mobile apps** | Kotlin/Swift/Dart (`apps/`) | Android, iOS, Flutter |
| **SDKs** | Multi-lang (`sdk/`) | Domain SDKs: AI, Game, Cloud, IoT |
| **Templates** | Static files (`templates/`) | Scaffolding cho `mg create-*` |

---

## 2. Thư mục gốc (Root)

```
MegaGate/
│
├── crates/                    # 🦀 Rust workspace — SSOT, package manager engine
│   ├── megagate-types/        #   Kiểu dữ liệu: config, package, lockfile, error, store, registry
│   ├── megagate-resolver/     #   Dependency resolution: PubGrub/semver, conflict detection
│   ├── megagate-linker/       #   Linking strategies: hardlink, symlink, copy
│   ├── megagate-extractor/    #   Content-addressable store (pnpm-style), tarball extraction
│   ├── megagate-fetcher/      #   HTTP fetching: registry client, rate-limited pool, retry
│   ├── megagate-security/     #   Security: typosquat, slopsquat, SBOM, provenance, lockdown
│   ├── megagate-core/         #   Orchestrator: resolve → fetch → link → lockfile pipeline
│   ├── megagate-cli/          #   Rust CLI binary: install/add/remove/list/audit/lock
│   ├── megagate-ffi/          #   FFI surface: UniFFI + NAPI-RS + WASM bindings
│   ├── megagate-proto/        #   Protobuf generated types (prost)
│   └── Cargo.toml             #   Workspace manifest
│
├── cli/                       # 🎯 TypeScript PM CLI — consumer của Rust core qua NAPI-RS
│   ├── src/                   #   PM engine (resolver, fetcher, linker, store, security...)
│   │   ├── commands/          #     CLI commands
│   │   │   ├── install.ts
│   │   │   ├── add.ts
│   │   │   ├── remove.ts
│   │   │   ├── list.ts
│   │   │   ├── audit.ts
│   │   │   ├── lock.ts
│   │   │   ├── create-web.ts     # mg create-web <name>
│   │   │   ├── create-app.ts     # mg create-app <name>
│   │   │   ├── create-game.ts    # mg create-game <name>
│   │   │   ├── create-ai.ts      # mg create-ai <name>
│   │   │   ├── create-cloud.ts   # mg create-cloud <name>
│   │   │   ├── create-iot.ts     # mg create-iot <name>
│   │   │   └── create-lib.ts     # mg create-lib <name>
│   │   ├── resolver/          #     Dependency resolution
│   │   ├── fetcher/           #     HTTP fetch + registry client
│   │   ├── linker/            #     Linking strategies
│   │   ├── store/             #     Content-addressable store
│   │   ├── security/          #     Security checks
│   │   ├── lockfile/          #     Lockfile operations
│   │   ├── installer/         #     Install orchestration
│   │   ├── config/            #     Config loading
│   │   ├── types/             #     Type definitions
│   │   ├── native/            #     NAPI-RS bridge
│   │   └── utils/             #     Helpers
│   ├── napi/                  #     NAPI-RS compiled binary
│   │   ├── index.js
│   │   ├── index.d.ts
│   │   └── megagate_core.node
│   ├── package.json
│   ├── tsconfig.json
│   └── tests/
│
├── web/                       # 🌐 Full-stack web application (backend + frontend)
│   ├── src/
│   │   ├── server.ts          #     HTTP server entry
│   │   ├── dev.ts             #     Dev server (HMR)
│   │   ├── build.ts           #     Production build
│   │   │
│   │   ├── api/               #     🔙 Backend: API route definitions
│   │   ├── service/           #     🔙 Backend: Business logic orchestration
│   │   ├── repository/        #     🔙 Backend: Data access layer
│   │   ├── domain/            #     🔗 Shared: Domain entities (Clean Architecture)
│   │   ├── config/            #     🔗 Shared: Application config
│   │   ├── util/              #     🔗 Shared: Utility functions
│   │   │
│   │   ├── app/               #     🖥 Frontend: SPA (React)
│   │   │   ├── main.tsx
│   │   │   ├── router.tsx
│   │   │   ├── components/    #       UI components
│   │   │   ├── hooks/         #       React hooks
│   │   │   ├── store/         #       State (Zustand)
│   │   │   ├── pages/         #       Route pages
│   │   │   └── styles/        #       CSS/theme
│   │   ├── shared/            #     🖥 Frontend: Shared UI code
│   │   └── types/             #     🖥 Frontend: UI-only types
│   │
│   ├── public/                #   Static assets (HTML, images, icons)
│   │   ├── index.html
│   │   ├── favicon.ico
│   │   └── static/
│   │
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   └── tests/
│       ├── unit/
│       └── integration/
│
├── apps/                      # 📱 Mobile & Desktop apps
│   ├── android/               #   Android app (Kotlin)
│   │   ├── app/
│   │   ├── build.gradle.kts
│   │   └── README.md
│   ├── ios/                   #   iOS app (Swift)
│   │   ├── Sources/
│   │   ├── Package.swift
│   │   └── README.md
│   └── flutter/               #   Flutter app (Dart)
│       ├── lib/
│       ├── pubspec.yaml
│       └── README.md
│
├── sdk/                       # 📦 Domain SDKs
│   ├── ai-agent/              #   AI Agent toolkit, MCP server
│   │   ├── src/
│   │   │   ├── tools.ts       #     MCP tool definitions
│   │   │   └── agent.ts       #     Agent integration
│   │   └── package.json
│   ├── game/                  #   Game engine plugins
│   │   ├── bevy/              #     Bevy plugin (Rust)
│   │   ├── godot/             #     Godot extension (GDScript/Rust)
│   │   └── unity/             #     Unity package (C#)
│   ├── cloud/                 #   Cloud serverless integration
│   │   ├── cloudflare/        #     Cloudflare Workers
│   │   ├── lambda/            #     AWS Lambda layer
│   │   └── k8s/               #     K8s sidecar gRPC
│   └── iot/                   #   IoT/Embedded targets
│       ├── arm/               #     ARM cross-compile config
│       └── riscv/             #     RISC-V cross-compile config
│
├── templates/                 # 📁 Project scaffolding templates
│   ├── web/                   #   mg create-web <name> [--template]
│   │   ├── vanilla/           #     HTML + TS + CSS
│   │   ├── react/             #     React + Vite
│   │   ├── next/              #     Next.js
│   │   └── vue/               #     Vue + Vite
│   ├── app/                   #   mg create-app <name> [--platform]
│   │   ├── kotlin/            #     Android app scaffold
│   │   ├── swift/             #     iOS app scaffold
│   │   └── flutter/           #     Flutter app scaffold
│   ├── game/                  #   mg create-game <name> [--engine]
│   │   ├── bevy/              #     Bevy game scaffold
│   │   ├── godot/             #     Godot project scaffold
│   │   └── unity/             #     Unity package scaffold
│   ├── ai/                    #   mg create-ai <name>
│   │   ├── agent/             #     AI agent project
│   │   └── mcp-server/        #     MCP server scaffold
│   ├── cloud/                 #   mg create-cloud <name> [--platform]
│   │   ├── cloudflare/        #     Cloudflare Worker
│   │   └── lambda/            #     AWS Lambda function
│   ├── iot/                   #   mg create-iot <name>
│   │   ├── embedded/          #     Embedded Rust project
│   │   └── firmware/          #     Firmware scaffold
│   └── lib/                   #   mg create-lib <name> [--lang]
│       ├── rust/              #     Rust library
│       ├── ts/                #     TypeScript library
│       └── python/            #     Python library
│
├── proto/                     # Protobuf schema definitions
│   └── megagate/v1/
│       ├── common.proto
│       ├── package.proto
│       ├── resolver.proto
│       ├── linker.proto
│       ├── extractor.proto
│       ├── fetcher.proto
│       ├── security.proto
│       ├── store.proto
│       └── lockfile.proto
│
├── bindings/                  # 🔗 Auto-generated language bindings
│   ├── ts/                    #   TypeScript (NAPI-RS)
│   ├── kotlin/                #   Kotlin (UniFFI JNI)
│   ├── swift/                 #   Swift (UniFFI)
│   ├── dart/                  #   Dart (FFI)
│   ├── python/                #   Python (PyO3)
│   ├── go/                    #   Go (CGO)
│   ├── cpp/                   #   C++ (C FFI)
│   ├── csharp/                #   C# (P/Invoke)
│   ├── zig/                   #   Zig (C FFI)
│   └── wasm/                  #   WASM (wasm-bindgen)
│
├── agent_memory/              # 🧠 AI agent memory store
│   ├── src/
│   │   ├── lib.rs             #   Global KV store (Trellis pattern)
│   │   ├── log.rs             #   Agent activity log
│   │   └── conversation.rs    #   Conversation manager
│   └── Cargo.toml
│
├── docs/                      # 📖 Documentation
│   ├── usage.md
│   └── tasks/
│
├── tools/                     # 🛠 Build & CI scripts
│   ├── build.sh
│   ├── release.sh
│   └── gen-bindings.sh
│
├── assets/                    # Logo, images
├── examples/                  # Usage examples
├── task/                      # Planning docs (gitignored)
│
├── Cargo.toml                 # Rust workspace root
├── CORE.md                    # File này
├── README.md
├── buf.gen.yaml               # Buf codegen config
├── buf.yaml                   # Buf schema registry
├── BUILD.bazel                # Bazel build
├── WORKSPACE.bazel
├── tsconfig.json              # TypeScript config (root)
└── .github/workflows/         # CI/CD pipelines
    └── ci.yml
```

---

## 3. CLI Command Tree

```
mg
├── install [dir]              # Cài dependencies (Rust core)
├── add <pkg> [--dev]          # Thêm dependency
├── update [pkg]               # Cập nhật
├── remove <pkg>               # Gỡ dependency
├── list [--graph] [--depth]   # Danh sách dependencies
├── audit                      # Audit security
├── lock
│   ├── verify                 # Xác thực lockfile integrity
│   └── export <format>        # Xuất lockfile (json/yaml)
│
├── create-web <name>          # Scaffold full-stack web project
├── create-app <name>          # Scaffold mobile/desktop app
├── create-game <name>         # Scaffold game project
├── create-ai <name>           # Scaffold AI agent project
├── create-cloud <name>        # Scaffold cloud serverless project
├── create-iot <name>          # Scaffold IoT/embedded project
└── create-lib <name>          # Scaffold library (Rust/TS/Python)

# Web app specific (chạy từ web/ directory)
mg dev                         # Dev server (full-stack: backend + frontend HMR)
mg build                       # Production build
mg start                       # Production server
```

### Command → Code mapping

| Command | Xử lý tại | Gọi |
|---------|-----------|-----|
| `mg install` | `cli/src/commands/install.ts` | Rust `megagate-core::install()` qua NAPI-RS |
| `mg add` | `cli/src/commands/add.ts` | Rust `megagate-core::add()` qua NAPI-RS |
| `mg create-web` | `cli/src/commands/create-web.ts` | Copy từ `templates/web/` + tạo full-stack scaffold |
| `mg create-game` | `cli/src/commands/create-game.ts` | Copy từ `templates/game/` |
| `mg dev` | `web/src/dev.ts` | Start dev server + HMR |
| `mg build` | `web/src/build.ts` | Build frontend + backend |
| `mg start` | `web/src/server.ts` | Production server (full-stack) |

---

## 4. Architecture Flow

```
Terminal                         TypeScript CLI                     Rust Core
───────────────────────────────────────────────────────────────────────────
$ mg install <dir>
        │
        ▼
  cli/src/commands/install.ts
        │
        ├── gọi native/napi.ts ───────────► napi/megagate_core.node
        │                                        │
        │                                        ▼
        │                                  crates/megagate-ffi
        │                                        │
        │                                        ▼
        │                                  crates/megagate-core
        │                                        │
        │                                 ┌──────┼──────┐
        │                                 ▼      ▼      ▼
        │                          resolver  fetcher  linker
        │                                 │      │      │
        │                                 ▼      ▼      ▼
        │                              megagate-lock.json
        │                                        │
        ◄─────────────────────────────────────────┘
        │
        ▼
  "Install completed: 42 added"

───────────────────────────────────────────────────────────────────────────
$ mg create-web my-app
        │
        ▼
  cli/src/commands/create-web.ts
        │
        ▼
  cp -r templates/web/react/ ./my-app/
        │
        ▼
  "Created my-app (full-stack: react + vite + api server)"

───────────────────────────────────────────────────────────────────────────
$ cd my-app && mg dev
        │
        ▼
  web/src/dev.ts
        │
        ├── backend server (api, service, repository)
        │       │
        │       ▼
        │   HTTP API (REST/GraphQL)
        │
        └── frontend dev server (HMR)
                │
                ▼
            Browser SPA (React)
```

## 5. `web/` — Full-stack Web Application

`web/` là một project hoàn chỉnh gồm **backend + frontend** trong cùng 1 codebase:

```
web/
├── src/
│   ├── server.ts          # 🔙 HTTP server entry (Express/Fastify)
│   ├── dev.ts             # 🔙 Dev server với HMR
│   ├── build.ts           # 🔙 Production build script
│   │
│   ├── api/               # 🔙 REST/GraphQL route handlers
│   │   ├── index.ts
│   │   ├── projects.ts
│   │   └── dependencies.ts
│   ├── service/           # 🔙 Business logic
│   │   ├── project-service.ts
│   │   └── dependency-service.ts
│   ├── repository/        # 🔙 Data access (DB, filesystem, cache)
│   │   ├── project-repo.ts
│   │   └── dependency-repo.ts
│   │
│   ├── domain/            # 🔗 Domain entities (dùng cho cả BE + FE)
│   │   ├── Project.ts
│   │   └── Dependency.ts
│   ├── config/            # 🔗 Config (dùng cho cả BE + FE)
│   ├── util/              # 🔗 Utilities (dùng cho cả BE + FE)
│   │
│   ├── app/               # 🖥 Frontend SPA (React)
│   │   ├── main.tsx
│   │   ├── router.tsx
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── store/
│   │   ├── pages/
│   │   └── styles/
│   ├── shared/            # 🖥 Frontend shared code
│   └── types/             # 🖥 Frontend UI types
│
├── public/                # Static assets
├── package.json           # 1 package.json cho cả BE + FE
├── tsconfig.json
└── vite.config.ts         # Vite cho frontend, tsx cho backend
```

---

### Luồng request trong full-stack web

```
Browser (React SPA)
    │  GET /api/projects
    ▼
web/src/server.ts
    │
    ▼
web/src/api/projects.ts         # Route handler
    │
    ▼
web/src/service/project-service.ts  # Business logic
    │
    ▼
web/src/repository/project-repo.ts  # Data access
    │
    ├── Database (SQLite/Postgres)
    ├── Rust core (qua NAPI-RS) cho PM operations
    └── File system
```

---

## 8. Domain Branches (Git Worktree)

| # | Branch | Worktree | AI Agent | Thư mục focus |
|---|--------|----------|----------|---------------|
| 1 | `sdk/web` | `MegaGate-web/` | Web-app AI | `cli/`, `web/`, `templates/web/` |
| 2 | `sdk/game` | `MegaGate-game/` | Game AI | `sdk/game/`, `templates/game/` |
| 3 | `ops/cicd` | `MegaGate-cicd/` | CICD AI | `.github/`, `tools/`, Bazel/Nix |
| 4 | `sdk/cloud` | `MegaGate-cloud/` | Cloud AI | `sdk/cloud/`, `templates/cloud/` |
| 5 | `sdk/ai` | `MegaGate-ai/` | AI Agent AI | `sdk/ai-agent/`, `agent_memory/` |
| 6 | `feature/iot` | `MegaGate-iot/` | IoT AI | `sdk/iot/`, `templates/iot/` |
| 7 | `feature/security` | `MegaGate-security/` | Security AI | `crates/megagate-security/`, `templates/` |
| 8 | `development` | `MegaGate-dev/` | Integration | Toàn bộ (merge test) |
| 9 | `main` | `MegaGate/` (gốc) | Stable | Chỉ rebase, không code trực tiếp |

---

## 9. Kiến trúc kết nối (Rust Core → Language Bindings)

```
                        ┌─────────────────────────┐
                        │   crates/megagate-core   │
                        │  (resolver, linker,      │
                        │   extractor, fetcher,    │
                        │   security)              │
                        └────────┬────────┬────────┘
                                 │        │
                    ┌────────────┘        └────────────┐
                    ▼                                   ▼
          ┌─────────────────┐                  ┌─────────────────┐
          │  megagate-ffi   │                  │  megagate-ffi   │
          │  (NAPI-RS)      │                  │  (UniFFI)       │
          └────────┬────────┘                  └────────┬────────┘
                   │                                     │
                   ▼                                     ├──────────────┐
          ┌─────────────────┐                            ▼              ▼
          │  cli/napi/      │                  ┌────────────┐  ┌────────────┐
          │  .node binary   │                  │ Kotlin/JNI │  │ Swift/FFI  │
          └────────┬────────┘                  │ (Android)  │  │ (iOS)      │
                   │                           └────────────┘  └────────────┘
                   ▼                                       ┌────────────┐
          ┌─────────────────┐                              │ Dart/FFI    │
          │  cli/src/       │                              │ (Flutter)   │
          │  TypeScript CLI │                              └────────────┘
          └─────────────────┘                              ┌────────────┐
                                                            │ Python/PyO3│
                   ▼                                        └────────────┘
          ┌─────────────────┐                              ┌────────────┐
          │  web/src/       │                              │ Go/CGO     │
          │  Web SPA        │                              └────────────┘
          └─────────────────┘                              ┌────────────┐
                                                            │ C++/C FFI  │
                                                            └────────────┘
```

---

## 12. Thư mục chi tiết mỗi crate (Rust workspace)

### 7.1 `crates/megagate-types`

```
megagate-types/
├── src/
│   ├── lib.rs
│   ├── config.rs             # MegagateConfig, LinkStrategy, WorkspaceConfig
│   ├── error.rs              # MegagateError enum
│   ├── package.rs            # PackageManifest, LockedPackage, ResolvedDependency
│   ├── lockfile.rs           # LockfileV1, ImporterDeps, StoreInfo
│   ├── registry.rs           # NpmPackageInfo, RegistryVersion, DistInfo
│   └── store.rs              # StoreBackend trait
```

### 7.2 `crates/megagate-resolver`

```
megagate-resolver/
├── src/
│   ├── lib.rs
│   ├── resolver.rs           # Resolver struct, resolve(), batch_resolve()
│   ├── graph.rs              # DependencyGraph, topological sort, cycle detection
│   └── conflict.rs           # Conflict detection, hoist/duplicate decisions
```

### 7.3 `crates/megagate-fetcher`

```
megagate-fetcher/
├── src/
│   ├── lib.rs
│   ├── fetcher.rs            # Fetcher struct, fetch(), fetch_multiple()
│   ├── pool.rs               # FetchPool (rate-limited concurrent downloads)
│   └── registry_client.rs   # NpmRegistryClient (metadata, tarball download)
```

### 7.4 `crates/megagate-linker`

```
megagate-linker/
├── src/
│   ├── lib.rs
│   ├── linker.rs             # Linker struct, link(), unlink(), clean()
│   └── strategy.rs           # HardlinkStrategy, SymlinkStrategy, CopyStrategy
```

### 7.5 `crates/megagate-extractor`

```
megagate-extractor/
├── src/
│   ├── lib.rs
│   ├── extractor.rs          # Extractor (tarball → store nodes)
│   └── store_backend.rs      # FsStoreBackend (v1/files, v1/nodes)
```

### 7.6 `crates/megagate-security`

```
megagate-security/
├── src/
│   ├── lib.rs
│   ├── typosquat.rs          # Levenshtein-based package name detection
│   ├── slopsquat.rs          # Scope/registry squat detection
│   ├── minimum_age.rs        # Minimum release age check
│   ├── approve_builds.rs     # Build script approval
│   ├── lockdown.rs           # Eval/addon detection
│   ├── provenance.rs         # Sigstore/SLSA verification
│   ├── sbom.rs               # CycloneDX 1.5 SBOM generation
│   └── manager.rs            # SecurityManager orchestrator
```

### 7.7 `crates/megagate-core`

```
megagate-core/
├── src/
│   └── lib.rs                # MegagateCore: install(), add(), update(), remove(),
│                                list(), audit(), verify_lockfile()
│                              # Orchestrates: resolver → fetcher → linker → lockfile
```

### 7.8 `crates/megagate-cli`

```
megagate-cli/
├── src/
│   └── main.rs               # CLI entry (clap): Install, Add, Update, Remove,
│                                List, Audit, Lock {Verify, Export}
```

### 7.9 `crates/megagate-ffi`

```
megagate-ffi/
├── src/
│   └── lib.rs                # #[uniffi::export] + #[napi] functions
│                              # Wraps all megagate-core operations
│                              # Triple-export: UniFFI + NAPI-RS + WASM
```

---

## 10. CLI Source (TypeScript) — Chi tiết

```
cli/
├── src/
│   ├── index.ts              # Entry: CLI parser (commander)
│   ├── commands/
│   │   ├── install.ts        # mg install [dir]
│   │   ├── add.ts            # mg add <pkg>
│   │   ├── remove.ts         # mg remove <pkg>
│   │   ├── list.ts           # mg list [--graph]
│   │   ├── audit.ts          # mg audit
│   │   ├── lock.ts           # mg lock verify/export
│   │   ├── create-web.ts     # mg create-web <name>
│   │   ├── create-app.ts     # mg create-app <name>
│   │   ├── create-game.ts    # mg create-game <name>
│   │   ├── create-ai.ts      # mg create-ai <name>
│   │   ├── create-cloud.ts   # mg create-cloud <name>
│   │   ├── create-iot.ts     # mg create-iot <name>
│   │   └── create-lib.ts     # mg create-lib <name>
│   │
│   ├── resolver/             # TS PM engine (sẽ migrate dần sang Rust)
│   ├── fetcher/              # HTTP fetch + registry client
│   ├── linker/               # Linking strategies
│   ├── store/                # Content-addressable store
│   ├── security/             # Security checks
│   ├── lockfile/             # Lockfile operations
│   ├── installer/            # Install orchestration
│   ├── config/               # Config loading
│   ├── types/                # Type definitions
│   │
│   ├── native/               # NAPI-RS bridge
│   │   ├── index.ts
│   │   └── types.ts
│   └── utils/                # Formatters, validators, constants
├── napi/
│   ├── index.js              # JS wrapper → megagate_core.node
│   ├── index.d.ts
│   └── megagate_core.node    # NAPI-RS compiled binary
├── package.json
├── tsconfig.json
└── tests/
    ├── unit/
    └── integration/
```

---

## 11. Web Full-stack — Chi tiết

```
web/
├── src/
│   │
│   │  ┌─────────────────────────────────────────────┐
│   │  │ 🔙 BACKEND (server, api, service, repository)│
│   │  └─────────────────────────────────────────────┘
│   │
│   ├── server.ts             # HTTP server entry (Express/Fastify/Hono)
│   ├── dev.ts                # Dev server + HMR
│   ├── build.ts              # Production build script
│   │
│   ├── api/                  # Route handlers
│   │   ├── index.ts          #   Router mount
│   │   ├── projects.ts       #   CRUD projects
│   │   └── dependencies.ts   #   Dependency management
│   ├── service/              # Business logic
│   │   ├── project-service.ts
│   │   └── dependency-service.ts
│   ├── repository/           # Data access
│   │   ├── project-repo.ts
│   │   └── dependency-repo.ts
│   │
│   │  ┌─────────────────────────────────────────────┐
│   │  │ 🔗 SHARED (domain, config, util)            │
│   │  │   Dùng cho cả backend + frontend            │
│   │  └─────────────────────────────────────────────┘
│   │
│   ├── domain/               # Domain entities
│   │   ├── Project.ts
│   │   ├── Dependency.ts
│   │   └── User.ts
│   ├── config/               # App config (env vars, constants)
│   ├── util/                 # Utilities
│   │
│   │  ┌─────────────────────────────────────────────┐
│   │  │ 🖥 FRONTEND (React SPA)                     │
│   │  └─────────────────────────────────────────────┘
│   │
│   ├── app/                  # SPA entry
│   │   ├── main.tsx
│   │   ├── router.tsx
│   │   ├── App.tsx
│   │   ├── components/       #   UI components
│   │   │   ├── common/       #     Button, Input, Modal...
│   │   │   ├── project/      #     Project card, list...
│   │   │   └── dependency/   #     Dep tree, version selector...
│   │   ├── hooks/            #   React hooks
│   │   │   ├── useDependencies.ts
│   │   │   ├── useProject.ts
│   │   │   └── useSDK.ts
│   │   ├── store/            #   Zustand stores
│   │   ├── pages/            #   Route pages
│   │   └── styles/           #   CSS/themes
│   ├── shared/               # Frontend shared code
│   └── types/                # UI-only types
│
├── public/                   # Static assets
│   ├── index.html
│   ├── favicon.ico
│   └── static/
│
├── package.json              # 1 package.json cho full-stack
├── tsconfig.json
├── vite.config.ts            # Vite (frontend) + tsx (backend)
└── tests/
    ├── unit/
    └── integration/

---

## 13. Templates — Quy ước

Mỗi template trong `templates/` là 1 thư mục chứa project mẫu hoàn chỉnh.

```
templates/
├── web/react/                # mg create-web my-app --template react
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       └── ...
│
├── app/kotlin/               # mg create-app my-app --platform android
│   ├── build.gradle.kts
│   └── app/src/main/...
│
└── game/bevy/                # mg create-game my-game --engine bevy
    ├── Cargo.toml
    └── src/main.rs
```

Khi chạy `mg create-web my-app --template react`, CLI sẽ:
1. Copy `templates/web/react/` → `./my-app/`
2. Replace template variables (`{{name}}` → `my-app`)
3. Init git repo
4. Chạy `npm install` (hoặc `cargo build`)

---

## 14. Supply Chain & Security

```
crates/megagate-security/
├── typosquat         # Levenshtein distance, common squat patterns
├── slopsquat         # Unofficial scope/registry detection
├── minimum_age       # Chặn package quá mới (default 24h)
├── approve_builds    # Whitelist build scripts
├── lockdown          # Ngăn eval/new Function/child_process
├── provenance        # Sigstore keyless signing + SLSA
└── sbom              # CycloneDX 1.5 export
```

---

## 15. Multi-Agent Worktree Strategy

Mỗi AI agent phụ trách 1 domain, làm việc trên 1 git worktree riêng:

```bash
# Setup worktrees
git worktree add ../MegaGate-web       sdk/web
git worktree add ../MegaGate-game      sdk/game
git worktree add ../MegaGate-cicd      ops/cicd
git worktree add ../MegaGate-cloud     sdk/cloud
git worktree add ../MegaGate-ai        sdk/ai
git worktree add ../MegaGate-iot       feature/iot
git worktree add ../MegaGate-security  feature/security
git worktree add ../MegaGate-dev       development

# Mỗi AI agent chỉ focus vào thư mục của domain mình
# AI-web       → cli/, web/, templates/web/
# AI-game      → sdk/game/, templates/game/
# AI-security  → crates/megagate-security/
```

---

## 16. Build & Run

```bash
# Rust core
cargo build                     # Build all crates
cargo test                      # Run all Rust tests
cargo run -- install            # Run mg install directly

# TypeScript CLI
cd cli && pnpm install && pnpm run build

# Web frontend
cd web && pnpm install && pnpm run dev

# NAPI-RS bridge rebuild
cd cli/napi && cargo build --release
```

---

## 17. License

MIT
