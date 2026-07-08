# MegaGate - Architecture Proposal (v2)
**Date**: 2026-07-08  
**Status**: DESIGN - Approved vision  
**Based on**: RESEARCH_REPORT.md findings + Competitive Analysis + User Discussion

---

## 1. EXECUTIVE SUMMARY

### Current State
- ❌ `/web/mg/` was isolated → now archived to `_archive/web-pm-v1/` (kept for reference)
- ❌ Old adapters had no structure → new 3-tier system (native/delegate/compiler)
- ❌ CLI had no wizard → new interactive `mg init` with per-core decision trees
- ❌ No scaffolding → new `cli/src/scaffold/` template processor

### Proposed Solution
```
✅ Unified Rust core (`core/`) — 80% code reuse
✅ 3-tier adapters (`adapters/`) — Native / Delegate / Compiler
✅ Unified CLI (`cli/`) — mg init (wizard) + mg install/add/remove/...
✅ Per-core wizard (`cli/src/wizard/`) — each core defines its decision tree
✅ Clean templates (`templates/`) — scaffolding for ALL ecosystems
✅ `/web/mg/` archived → `_archive/web-pm-v1/` (40k lines preserved)
```

---

## 2. NEW FOLDER STRUCTURE


```
MegaGate/                                # 🏠 Root
│
├── .github/workflows/                   # CI/CD (GitHub Actions)
│   ├── ci.yml                           # Main CI pipeline
│   ├── release.yml                      # Release automation
│   └── test-all-cores.yml               # Test all adapters
│
├── core/                                # 🦀 Shared Rust Core (80% reuse)
│   ├── Cargo.toml                       # Workspace root
│   ├── crates/
│   │   ├── mg-http/                     # HTTP client (reqwest wrapper)
│   │   ├── mg-store/                    # Content-addressable store (CAS)
│   │   ├── mg-crypto/                   # SHA-256, integrity verification
│   │   ├── mg-lockfile/                 # Unified lockfile format
│   │   ├── mg-resolver/                 # PubGrub resolver (generic)
│   │   ├── mg-fetcher/                  # Parallel download pool
│   │   ├── mg-ui/                       # TUI components (ratatui)
│   │   ├── mg-config/                   # Config parsing (TOML/YAML)
│   │   └── mg-types/                    # Shared types, traits, errors
│   └── README.md                        # Core architecture docs
│
├── adapters/                            # 🔌 Ecosystem-Specific Adapters
│   ├── web/                             # Web PM (npm, pnpm compatibility)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   ├── lib.rs                   # Web adapter (auto-detect + dispatch)
│   │   ├── native/                  # 🟢 Rust-native (npm)
│   │   │   ├── npm_registry.rs      # npm registry client
│   │   │   ├── resolver.rs          # PubGrub resolver
│   │   │   └── node_modules.rs      # smart linker
│   │   ├── delegate/                # 🟡 Wraps existing PMs
│   │   │   ├── composer.rs          # PHP/Composer
│   │   │   ├── maven.rs             # Java/Maven
│   │   │   └── go_mod.rs            # Go modules
│   │   ├── compiler/                # 🔵 Build pipelines
│   │   │   ├── vite.rs              # Vite bundler
│   │   │   └── next.rs              # Next.js build
│   │   └── tests/
│   │
│   ├── game/                            # Game engines adapter
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── bevy.rs                  # Bevy (Cargo-based)
│   │   │   ├── unity.rs                 # Unity UPM
│   │   │   ├── unreal.rs                # Unreal .uplugin
│   │   │   ├── godot.rs                 # Godot asset library
│   │   │   └── lib.rs
│   │   └── tests/
│   │

│   ├── ai/                              # AI/ML adapter
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── pypi.rs                  # PyPI registry
│   │   │   ├── conda.rs                 # Conda packages
│   │   │   ├── huggingface.rs           # HuggingFace models
│   │   │   ├── venv.rs                  # Virtual env management
│   │   │   └── lib.rs
│   │   └── tests/
│   │
│   ├── cloud/                           # Cloud IaC adapter
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── pulumi.rs                # Pulumi wrapper
│   │   │   ├── terraform.rs             # Terraform wrapper
│   │   │   ├── cdk.rs                   # AWS CDK
│   │   │   └── lib.rs
│   │   └── tests/
│   │
│   ├── iot/                             # IoT/Embedded adapter
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── cargo_embedded.rs        # Cargo for embedded Rust
│   │   │   ├── platformio.rs            # PlatformIO wrapper
│   │   │   ├── zephyr_west.rs           # Zephyr West wrapper
│   │   │   ├── flash.rs                 # Flash tool integration
│   │   │   └── lib.rs
│   │   └── tests/
│   │
│   └── README.md                        # Adapter development guide
│
├── cli/                                 # 🎯 Unified CLI Binary
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs                      # Entry point (clap CLI)
│   │   ├── commands/
│   │   │   ├── init.rs                  # mg init (wizard orchestrator)
│   │   │   ├── install.rs               # mg install
│   │   │   ├── add.rs                   # mg add <pkg>
│   │   │   ├── remove.rs                # mg remove <pkg>
│   │   │   ├── update.rs                # mg update
│   │   │   ├── list.rs                  # mg list
│   │   │   ├── info.rs                  # mg info <pkg>
│   │   │   └── search.rs                # mg search <query>
│   │   ├── wizard/                      # 🧙 Interactive project wizard
│   │   │   ├── mod.rs                   # Module exports
│   │   │   ├── engine.rs                # Generic TUI flow engine
│   │   │   ├── web.rs                   # Web core decision tree
│   │   │   ├── game.rs                  # Game core (future)
│   │   │   ├── ai.rs                    # AI core (future)
│   │   │   ├── cloud.rs                 # Cloud core (future)
│   │   │   └── iot.rs                   # IoT core (future)
│   │   └── scaffold/                    # 🏗 Project scaffolding
│   │       ├── mod.rs
│   │       └── processor.rs             # Template processing + file gen
│
├── templates/                           # 📁 Project Scaffolding Templates
│   ├── web/
│   │   ├── vanilla/                     # HTML + CSS + TypeScript
│   │   │   ├── index.html
│   │   │   ├── style.css
│   │   │   ├── main.ts
│   │   │   └── package.json.tmpl        # Template with {{name}}
│   │   ├── react-vite/                  # React + Vite
│   │   ├── next-app/                    # Next.js App Router
│   │   ├── vue-vite/                    # Vue 3 + Vite
│   │   └── svelte/                      # SvelteKit
│   │
│   ├── game/
│   │   ├── bevy/                        # Bevy (Rust)
│   │   │   ├── Cargo.toml.tmpl
│   │   │   └── src/main.rs
│   │   ├── unity/                       # Unity project
│   │   │   ├── manifest.json.tmpl
│   │   │   └── Packages/
│   │   ├── unreal/                      # Unreal project
│   │   │   └── .uproject.tmpl
│   │   └── godot/                       # Godot project
│   │       └── project.godot.tmpl
│   │
│   ├── ai/
│   │   ├── python-agent/                # Python AI agent
│   │   │   ├── pyproject.toml.tmpl
│   │   │   ├── requirements.txt
│   │   │   └── src/agent.py
│   │   └── mcp-server/                  # MCP server template
│   │       └── server.py
│   │
│   ├── cloud/
│   │   ├── pulumi-aws/                  # Pulumi + AWS

│   │   ├── terraform-gcp/               # Terraform + GCP
│   │   └── cdk-typescript/              # AWS CDK TypeScript
│   │
│   ├── iot/
│   │   ├── esp32-rust/                  # ESP32 embedded Rust
│   │   │   ├── Cargo.toml.tmpl
│   │   │   └── .cargo/config.toml
│   │   └── zephyr-arm/                  # Zephyr RTOS
│   │       └── west.yml.tmpl
│   │
│   └── lib/
│       ├── rust/                        # Rust library template
│       └── typescript/                  # TypeScript library
│
├── docs/                                # 📖 Documentation
│   ├── getting-started.md               # Quick start guide
│   ├── architecture.md                  # System architecture
│   ├── adapters/                        # Adapter development guides
│   │   ├── web.md
│   │   ├── game.md
│   │   ├── ai.md
│   │   ├── cloud.md
│   │   └── iot.md
│   ├── cli-reference.md                 # CLI commands reference
│   └── contributing.md                  # Contribution guidelines
│
├── examples/                            # 💡 Usage Examples
│   ├── web-react-app/                   # Example React app using mg
│   ├── bevy-game/                       # Example Bevy game
│   ├── python-ai-agent/                 # Example AI agent
│   └── README.md
│
├── scripts/                             # 🛠 Build & CI Scripts
│   ├── build.sh                         # Build all cores
│   ├── test.sh                          # Run all tests
│   ├── release.sh                       # Release automation
│   └── gen-docs.sh                      # Generate documentation
│
├── _archive/                            # 🗄 Legacy Code (DO NOT DELETE)
│   └── web-pm-v1/                       # Original /web/mg/ (moved here)
│       ├── README.md                    # "This is archived v1"
│       ├── Cargo.toml
│       ├── crates/

│       │   ├── mg-core/
│       │   ├── mg-resolver/
│       │   ├── mg-lockfile/
│       │   └── ... (all 40k+ lines)
│       └── NOTE.md                      # "Kept for code extraction"
│
├── assets/                              # 🎨 Logo & Branding
│   ├── logo.svg
│   ├── logo-in.svg
│   └── favicon.ico
│
├── Cargo.toml                           # 🦀 Rust Workspace Root
├── Cargo.lock
├── README.md                            # 📝 Main README
├── LICENSE                              # MIT License
├── CONTRIBUTING.md                      # How to contribute
├── CHANGELOG.md                         # Version history
├── RESEARCH_REPORT.md                   # This research (already created)
├── ARCHITECTURE_PROPOSAL.md             # This file
└── .gitignore

```

---

## 3. MIGRATION PLAN

### Step 1: Archive Current `/web/mg/`
```bash
# DO NOT DELETE - Just move to archive
mkdir -p _archive/
mv web/mg/ _archive/web-pm-v1/
echo "# Archived Web PM v1\n\nThis is the original implementation. Kept for reference and code extraction." > _archive/web-pm-v1/NOTE.md
```

### Step 2: Create New Structure
```bash
# Core
mkdir -p core/crates/{mg-http,mg-store,mg-crypto,mg-lockfile,mg-resolver,mg-fetcher,mg-ui,mg-config,mg-types}

# Adapters
mkdir -p adapters/{web,game,ai,cloud,iot}/src

# CLI
mkdir -p cli/src/commands

# Templates
mkdir -p templates/{web,game,ai,cloud,iot,lib}

# Docs
mkdir -p docs/adapters

# Examples
mkdir -p examples

# Scripts
mkdir -p scripts
```


### Step 3: Extract Reusable Code from Archive
```bash
# From _archive/web-pm-v1/crates/mg-store/ 
#   → Copy to core/crates/mg-store/

# From _archive/web-pm-v1/crates/mg-resolver/
#   → Copy to core/crates/mg-resolver/

# From _archive/web-pm-v1/crates/mg-lockfile/
#   → Copy to core/crates/mg-lockfile/

# From _archive/web-pm-v1/crates/mg-registry/
#   → Refactor to adapters/web/src/npm_registry.rs
```

### Step 4: Clean Up Old Folders
```bash
# Remove empty/placeholder folders
rm -rf sdk/ apps/ packages/ bindings/ memanto/ proto/

# Keep these:
# - task/ (planning docs)
# - docs/ (documentation)
# - assets/ (logo)
# - .github/ (CI/CD)
```

---

## 4. CLI ARCHITECTURE

### Commands Tree
```
mg
├── init                                 # Interactive wizard (entry point)
├── install [dir]                        # Install dependencies
├── add <pkg> [--dev]                    # Add dependency
├── update [pkg]                         # Update dependencies
├── remove <pkg>                         # Remove dependency
├── list [--tree] [--depth N]            # List dependencies
├── info <pkg>                           # Package info
├── search <query>                       # Search packages
├── ui                                   # Launch TUI dashboard
└── --version                            # Show version
```

### `mg init` — Web Core Decision Tree (Full)

```
mg init
  └── 🌐 Web
       └── Type?
            ├── Frontend
            │    └── Framework?
            │         ├── Next.js
            │         ├── React + Vite
            │         ├── Vue + Vite
            │         ├── Nuxt
            │         ├── SvelteKit
            │         ├── Angular
            │         ├── Solid.js
            │         ├── Qwik
            │         ├── Vanilla (HTML + TS)
            │         └── Astro
            │
            ├── Backend
            │    └── Language?
            │         ├── Node.js / TS
            │         │    └── Express / Fastify / NestJS / Hono / tRPC
            │         ├── PHP
            │         │    └── Laravel / Symfony
            │         ├── Java
            │         │    └── Spring Boot / Quarkus
            │         ├── Go
            │         │    └── Gin / Echo / Fiber
            │         ├── Python
            │         │    └── FastAPI / Django / Flask
            │         └── Rust
            │              └── Axum / Actix-Web
            │
            ├── Fullstack
            │    └── Stack?
            │         ├── Next.js (FE+BE all-in-one)
            │         ├── Nuxt (FE+BE all-in-one)
            │         ├── SvelteKit (FE+BE all-in-one)
            │         ├── Remix (FE+BE all-in-one)
            │         ├── React + Fastify (separate)
            │         ├── Vue + Laravel (separate)
            │         └── Custom (pick your own)
            │
            └── Monorepo
                 ├── FE framework?
                 │    ├── Next.js / React / Vue / SvelteKit / Vanilla
                 └── BE framework?
                      ├── NestJS / Express / Fastify / Laravel
                      ├── Spring Boot / Gin / FastAPI / Axum
                      └──

=== After framework selection ===

            └── Project name?
            │    [my-app]
            └── Features?
                 ├─ [1] Use default settings (recommended)
                 └─ [2] Customize per framework
                      ├── FE: TypeScript, Tailwind, ESLint, Vitest...
                      └── BE: TypeScript, Prisma, Jest, Swagger...

            └── ✅ Generating...
                 ├── mg scaffold (copy templates + replace vars)
                 ├── mg install (run appropriate adapter)
                 └── Done!
```

### Architecture: Wizard Engine (`cli/src/wizard/`)

```
wizard/
├── mod.rs            # Module exports
├── engine.rs         # WizardEngine — generic TUI flow
│   ├── Question enum (Select / MultiSelect / Input)
│   ├── Answer struct (value + next_questions)
│   └── run_question() — renders via dialoguer/ratatui
│
├── web.rs            # WebWizard — Web core decision tree
│   └── build_tree() → Question tree with ALL frameworks
│
├── game.rs           # GameWizard (future)
├── ai.rs             # AiWizard (future)
├── cloud.rs          # CloudWizard (future)
└── iot.rs            # IotWizard (future)
```

Each core wizard:
- Defines its own decision tree (frameworks, features, templates)
- Returns `ScaffoldConfig` after completion
- Uses shared `WizardEngine` for TUI rendering
  🔧 Installing dependencies...
  ✅ Done in 2.3s!

Next steps:
  cd my-awesome-app
  mg dev            # Start dev server
```

---

## 5. CODE REUSE STRATEGY

### From `/web/mg/` Archive

| Source Crate | Destination | Reuse % | Notes |
|--------------|-------------|---------|-------|
| `mg-core/src/cffi/sha256.rs` | `core/crates/mg-crypto/` | 100% | SHA-256 implementation |
| `mg-store/` | `core/crates/mg-store/` | 95% | Content-addressable store |
| `mg-resolver/` | `core/crates/mg-resolver/` | 80% | Refactor to generic |
| `mg-lockfile/` | `core/crates/mg-lockfile/` | 90% | Unified format |
| `mg-registry/` | `adapters/web/src/npm_registry.rs` | 70% | npm-specific |
| `mg-fetcher/` | `core/crates/mg-fetcher/` | 100% | Generic HTTP download |
| `mg-installer/` | `adapters/web/src/node_modules.rs` | 60% | Web-specific linker |
| `mg-cli/` | `cli/` | 30% | Rewrite for multi-core |

### New Components to Build


| Component | Location | Priority | Estimated LOC |
|-----------|----------|----------|---------------|
| Adapter trait | `core/crates/mg-types/src/adapter.rs` | P0 | 200 |
| TUI dashboard | `core/crates/mg-ui/` | P1 | 500 |
| Game adapter (Bevy) | `adapters/game/src/bevy.rs` | P1 | 400 |
| AI adapter (PyPI) | `adapters/ai/src/pypi.rs` | P1 | 600 |
| Cloud adapter (Pulumi) | `adapters/cloud/src/pulumi.rs` | P2 | 300 |
| IoT adapter (Cargo embedded) | `adapters/iot/src/cargo_embedded.rs` | P2 | 400 |
| Interactive `mg init` | `cli/src/commands/init.rs` | P0 | 800 |

---

## 6. GIT WORKFLOW

### Branch Strategy
```
main                         # Production releases only
  ↓
development                  # Integration branch (default)
  ↓
  ├─ feature/web-adapter     # Web development
  ├─ feature/game-adapter    # Game development
  ├─ feature/ai-adapter      # AI development
  ├─ feature/cloud-adapter   # Cloud development
  ├─ feature/iot-adapter     # IoT development
  ├─ feature/cli-refactor    # CLI refactor
  └─ feature/docs            # Documentation
```

### CI/CD Pipeline (`.github/workflows/ci.yml`)
```yaml
name: CI

on:
  push:
    branches: [development, main]
  pull_request:
    branches: [development]

jobs:
  test-core:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - name: Test core crates
        run: |
          cd core
          cargo test --all-features

  test-adapters:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        adapter: [web, game, ai, cloud, iot]
    steps:
      - uses: actions/checkout@v4
      - name: Test ${{ matrix.adapter }} adapter
        run: |
          cd adapters/${{ matrix.adapter }}
          cargo test

  test-cli:
    runs-on: ubuntu-latest
    steps:
      - name: Test CLI
        run: |
          cd cli
          cargo test
          cargo build --release

  integration-test:
    needs: [test-core, test-adapters, test-cli]
    runs-on: ubuntu-latest
    steps:
      - name: Integration tests
        run: cargo test --all
```


---

## 7. ADAPTER TRAIT DESIGN + 3-TIER SYSTEM

### 3 Adapter Modes

MegaGate supports 3 adapter modes depending on the ecosystem:

| Mode | Approach | Rust resolver? | Use Case | Examples |
|------|----------|----------------|----------|----------|
| 🟢 **Native** | Rust tự implement | ✅ Yes | Làm tốt hơn PM hiện tại | npm, PyPI, Cargo |
| 🟡 **Delegate** | Wrap tool gốc | ❌ No (gọi binary) | Hợp nhất CLI, không thay thế | composer, maven, go mod |
| 🔵 **Compiler** | Build pipeline | ❌ No (gọi build tool) | Build/bundle/optimize | vite build, next build, tsc |

```rust
/// Adapter mode — determines how the adapter operates
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterMode {
    /// Rust-native: tự resolve, fetch, store, link
    Native,
    /// Delegate: wrap existing package manager binary
    Delegate { binary: String, manifest: String },
    /// Compiler: build/bundle pipeline
    Compiler { build_system: BuildSystem },
}
```

### Core Trait (`core/crates/mg-types/src/adapter.rs`)
```rust
use async_trait::async_trait;
use std::path::Path;

#[async_trait]
pub trait PackageAdapter: Send + Sync {
    /// Adapter name (e.g., "web", "game", "ai")
    fn name(&self) -> &str;
    
    /// Adapter mode
    fn mode(&self) -> AdapterMode;
    
    /// Detect if this adapter can handle the project
    fn can_handle(&self, project_root: &Path) -> bool;
    
    /// Parse project manifest (package.json, Cargo.toml, etc.)
    async fn parse_manifest(&self, project_root: &Path) -> Result<Manifest>;
    
    /// Resolve dependencies
    async fn resolve(&self, manifest: &Manifest) -> Result<ResolvedGraph>;
    
    /// Fetch packages
    async fn fetch(&self, packages: &[Package]) -> Result<()>;
    
    /// Install/link packages
    async fn install(&self, packages: &[Package], target: &Path) -> Result<()>;
    
    /// Update specific package
    async fn update(&self, package: &str, version: Option<&str>) -> Result<()>;
    
    /// Remove package
    async fn remove(&self, package: &str) -> Result<()>;
    
    /// List installed packages
    async fn list(&self) -> Result<Vec<InstalledPackage>>;
    
    /// Security audit
    async fn audit(&self) -> Result<AuditReport>;
}
```

### Native vs Delegate — Ví dụ Code

**Native (npm/JS):**
```rust
impl PackageAdapter for WebJsAdapter {
    fn mode(&self) -> AdapterMode { AdapterMode::Native }
    
    async fn resolve(&self, deps: &[Dep]) -> Result<ResolvedGraph> {
        // PubGrub algorithm — tự viết, NHANH hơn npm
        self.resolver.solve(deps).await
    }
}
```

**Delegate (PHP/Composer):**
```rust
impl PackageAdapter for WebPhpAdapter {
    fn mode(&self) -> AdapterMode { 
        AdapterMode::Delegate { 
            binary: "composer".into(), 
            manifest: "composer.json".into() 
        }
    }
    
    async fn install(&self) -> Result<()> {
        // Gọi composer install, parse output
        Command::new("composer")
            .arg("install")
            .arg("--no-interaction")
            .output()?;
        // Parse composer.lock → mg.lock
        self.sync_lockfile().await
    }
}
```

### Ecosystem-Specific Languages

Mỗi core dùng ngôn ngữ phù hợp với ecosystem của nó:

| Core | Engine/CLI | Templates | Rationale |
|------|-----------|-----------|-----------|
| **Web** | Rust + Zig + C | JS/TS, PHP, Java, Go, Python, Rust | Tối ưu speed cho npm; delegate cho phần còn lại |
| **Game** | Rust core | C++, C# | Unity = C#, Unreal = C++, Bevy = Rust |
| **AI** | Rust (CLI) + Python (runtime) | Python | uv model: CLI nhanh, runtime Python |
| **Cloud** | Rust core | TypeScript, Go | Pulumi multi-lang, Terraform Go |
| **App** | Rust core | Kotlin, Swift, Dart, TS | Platform-native mobile |
| **IoT** | Rust core | Rust, C | Embedded-first, `no_std` |

### Example: Web Adapter Implementation
```rust
// adapters/web/src/lib.rs
use mg_types::{PackageAdapter, Manifest, Result};
use std::path::Path;

pub struct WebAdapter {
    http: mg_http::Client,
    store: mg_store::Store,
    resolver: mg_resolver::Resolver,
}

#[async_trait]
impl PackageAdapter for WebAdapter {
    fn name(&self) -> &str { "web" }
    
    fn can_handle(&self, project_root: &Path) -> bool {
        project_root.join("package.json").exists()
    }
    
    async fn parse_manifest(&self, project_root: &Path) -> Result<Manifest> {
        let pkg_json = project_root.join("package.json");
        let content = tokio::fs::read_to_string(&pkg_json).await?;
        let parsed: PackageJson = serde_json::from_str(&content)?;
        Ok(Manifest::from_package_json(parsed))
    }
    
    // ... implement other methods
}
```

---

## 8. TEMPLATE SYSTEM

### Template Variables
Templates support Handlebars-style variables:
```
{{name}}           # Project name
{{author}}         # Author name (from git config)
{{version}}        # Initial version (default: 0.1.0)
{{description}}    # Project description
{{license}}        # License (default: MIT)
{{year}}           # Current year
```

### Example: `templates/web/react-vite/package.json.tmpl`
```json
{
  "name": "{{name}}",
  "version": "{{version}}",
  "description": "{{description}}",
  "author": "{{author}}",
  "license": "{{license}}",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "latest",
    "react-dom": "latest"
  },
  "devDependencies": {
    "@types/react": "latest",
    "@types/react-dom": "latest",
    "@vitejs/plugin-react": "latest",
    "typescript": "latest",
    "vite": "latest"
  }
}
```

**NOTE**: Templates use `"latest"` by default. MegaGate resolves to actual versions from:
- npm registry (latest tag)
- GitHub releases (for opensource packages)
- User can override: `mg add react@18.2.0`

### Template Processing (`cli/src/template.rs`)
```rust
use handlebars::Handlebars;
use std::collections::HashMap;

pub struct TemplateProcessor {
    registry: Handlebars<'static>,
}

impl TemplateProcessor {
    pub fn new() -> Self {
        Self { registry: Handlebars::new() }
    }
    
    pub fn process(&self, template: &str, vars: &HashMap<String, String>) -> Result<String> {
        self.registry.render_template(template, vars)
            .map_err(|e| Error::TemplateError(e.to_string()))
    }
    
    pub fn process_dir(&self, template_dir: &Path, target_dir: &Path, vars: &HashMap<String, String>) -> Result<()> {
        for entry in walkdir::WalkDir::new(template_dir) {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension() == Some("tmpl") {
                let content = fs::read_to_string(path)?;
                let processed = self.process(&content, vars)?;
                let target = target_dir.join(path.strip_prefix(template_dir)?);
                fs::write(target, processed)?;
            } else {
                // Copy non-template files as-is
                fs::copy(path, target_dir.join(path.strip_prefix(template_dir)?))?;
            }
        }
        Ok(())
    }
}
```

---

## 9. UI/UX DESIGN

### Terminal UI (TUI) Dashboard
Using `ratatui` crate for rich terminal interface:

```
┌──────────────────────────────────────────────────────────────────────┐
│ MegaGate Dashboard                                       [q] Quit    │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  📊 Project Overview                                                 │
│  ├─ Type: Web (React + Vite)                                        │
│  ├─ Dependencies: 342 packages                                      │
│  ├─ Dev Dependencies: 89 packages                                   │
│  └─ Disk Usage: 1.2 GB (CAS: 450 MB saved)                         │
│                                                                      │
│  🔄 Recent Activity                                                  │
│  ├─ 2 minutes ago: Added @types/node@20.10.0                       │
│  ├─ 1 hour ago: Updated react@18.2.0 → 18.3.0                      │
│  └─ Today 10:23: Installed 342 packages in 3.2s                    │
│                                                                      │
│  🛡️  Security Status: ✅ No vulnerabilities                         │
│                                                                      │
│  ⚡ Performance                                                      │
│  ├─ Install Speed: 3.2s (10x faster than npm)                      │
│  ├─ Cache Hit Rate: 94%                                             │
│  └─ Network: 12 MB downloaded, 450 MB cached                        │
│                                                                      │
│  [Tab] Switch View  [↑↓] Navigate  [Enter] Details  [h] Help       │
└──────────────────────────────────────────────────────────────────────┘
```

### Progress Indicators
```
Installing dependencies...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 342/342 packages
├─ react@18.3.0 ✓
├─ react-dom@18.3.0 ✓
├─ vite@5.4.0 ✓
└─ ... (339 more)

✓ Done in 3.2s (10x faster than npm)
```

---

## 10. SECURITY FEATURES

### Supply Chain Security (Inherited from `/web/mg/`)
```
adapters/web/src/security/
├── typosquat.rs              # Detect typosquatting (e.g., "reaact")
├── dependency_confusion.rs   # Prevent internal pkg shadowing
├── integrity.rs              # SHA-256 verification
├── provenance.rs             # Sigstore/SLSA attestation
└── audit.rs                  # CVE scanning
```

### Security Audit Output
```bash
$ mg audit

🛡️  Security Audit Report
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✅ No critical vulnerabilities found

⚠️  2 moderate vulnerabilities:
├─ axios@0.21.1 (GHSA-cph5-m8f7-6c5x)
│  └─ Fixed in: 0.21.2
│  └─ Run: mg update axios
└─ lodash@4.17.19 (CVE-2020-8203)
   └─ Fixed in: 4.17.21
   └─ Run: mg update lodash

📦 342 packages audited
⏱️  Completed in 0.8s
```

---

## 11. PERFORMANCE TARGETS

### Benchmarks (Goal)
| Operation | npm | pnpm | bun | **mg (target)** |
|-----------|-----|------|-----|----------------|
| Fresh install (342 pkgs) | 60s | 15s | 4s | **3s** |
| Cached install | 30s | 5s | 0.8s | **0.5s** |
| Add single package | 8s | 3s | 0.5s | **0.3s** |
| Lockfile generation | 2s | 1s | 0.2s | **0.1s** |

### Optimization Strategies
1. **Rust core**: Zero-cost abstractions, no GC pauses
2. **Parallel fetching**: 50 concurrent downloads
3. **CAS deduplication**: pnpm-style content-addressable store
4. **Smart caching**: DashMap for in-memory cache
5. **Streaming extraction**: No temp files, direct to store

---

## 12. DOCUMENTATION STRUCTURE

### `docs/` Organization
```
docs/
├── getting-started.md         # 5-minute quickstart
├── installation.md            # Install instructions
├── architecture.md            # System design overview
│
├── guides/
│   ├── web-development.md     # Web PM guide
│   ├── game-development.md    # Game engine integration
│   ├── ai-development.md      # AI/ML workflows
│   ├── cloud-deployment.md    # IaC guide
│   └── iot-embedded.md        # Embedded/IoT guide
│
├── cli-reference.md           # Command reference
│
├── adapters/
│   ├── creating-adapter.md    # How to create new adapter
│   ├── web-adapter.md         # Web adapter internals
│   ├── game-adapter.md        # Game adapter internals
│   └── ...
│
├── api/
│   ├── core-api.md            # Core crates API docs
│   └── adapter-trait.md       # Adapter trait reference
│
└── contributing.md            # Contribution guidelines
```

---

## 13. IMPLEMENTATION PHASES

### Phase 1: Foundation (Week 1-2) - PRIORITY
- [ ] Create new folder structure
- [ ] Move `/web/mg/` → `_archive/web-pm-v1/`
- [ ] Extract reusable code to `core/`
  - [ ] `mg-http` (HTTP client)
  - [ ] `mg-store` (CAS)
  - [ ] `mg-crypto` (SHA-256)
  - [ ] `mg-lockfile` (format)
  - [ ] `mg-resolver` (PubGrub)
  - [ ] `mg-fetcher` (parallel download)
- [ ] Define `PackageAdapter` trait
- [ ] Build unified CLI skeleton (`mg init`, `mg create-*`)

### Phase 2: Web Adapter (Week 3)
- [ ] Migrate web PM logic to `adapters/web/`
- [ ] npm registry client
- [ ] package.json parser
- [ ] node_modules linker
- [ ] Test suite (ensure 811 tests still pass)

### Phase 3: Templates & Scaffolding (Week 4)
- [ ] Create web templates (vanilla, react, next, vue)
- [ ] Implement template processor
- [ ] Test `mg create-web` command
- [ ] Document template system

### Phase 4: Game Adapter (Week 5-6)
- [ ] Bevy adapter (Cargo-based)
- [ ] Unity adapter (UPM packages)
- [ ] Game templates
- [ ] Test `mg create-game`

### Phase 5: AI Adapter (Week 7)
- [ ] PyPI client
- [ ] Virtual env management
- [ ] Model cache (HuggingFace)
- [ ] Test `mg create-ai`

### Phase 6: Cloud & IoT (Week 8+)
- [ ] Cloud adapters (Pulumi, Terraform)
- [ ] IoT adapter (embedded Rust)
- [ ] Complete templates

### Phase 7: Polish (Week 9+)
- [ ] TUI dashboard (`mg ui`)
- [ ] Performance optimization
- [ ] Documentation
- [ ] CI/CD pipeline
- [ ] Release v1.0.0

---

## 14. TESTING STRATEGY

### Unit Tests
```bash
# Core crates (isolated)
cd core/crates/mg-store && cargo test
cd core/crates/mg-resolver && cargo test

# Adapters
cd adapters/web && cargo test
cd adapters/game && cargo test
```

### Integration Tests
```bash
# Full CLI workflow
cargo test --all --test integration_tests

# Example: Test web project creation
#[test]
fn test_create_web_react_project() {
    let temp = tempdir()?;
    run_mg(&["create-web", "my-app", "--template", "react"], temp)?;
    assert!(temp.join("my-app/package.json").exists());
    assert!(temp.join("my-app/src/App.tsx").exists());
}
```

### E2E Tests (Real Projects)
```bash
# Test on real npm packages
./scripts/test-real-projects.sh
  - Install react-dom (500+ dependencies)
  - Install next.js (300+ dependencies)
  - Verify lockfile integrity
  - Benchmark vs npm/pnpm/bun
```

---

## 15. NEXT STEPS FOR USER

### ⚠️ IMPORTANT: User Approval Required

**BẠN CẦN XÁC NHẬN:**

1. ✅ **Folder structure mới có OK không?**
   - `core/` cho shared code
   - `adapters/` cho ecosystem-specific
   - `_archive/web-pm-v1/` giữ code cũ

2. ✅ **Di chuyển `/web/mg/` sang archive?**
   - Command: `mv web/mg/ _archive/web-pm-v1/`
   - Keep for reference, extract reusable code

3. ✅ **Xóa folders trống?**
   - `sdk/`, `apps/`, `packages/`, `bindings/`, `proto/`

4. ✅ **Git workflow OK?**
   - `main` → `development` → feature branches

**SAU KHI BẠN CONFIRM**, tôi sẽ:
1. Tạo migration script (`scripts/migrate.sh`)
2. Commit RESEARCH_REPORT.md + ARCHITECTURE_PROPOSAL.md
3. Thực hiện migration (nếu bạn muốn)

---

## 16. ESTIMATED EFFORT

| Phase | Duration | Team Size | LOC | Priority |
|-------|----------|-----------|-----|----------|
| Phase 1: Foundation | 2 weeks | 1-2 devs | 5,000 | **P0** |
| Phase 2: Web Adapter | 1 week | 1 dev | 3,000 | **P0** |
| Phase 3: Templates | 1 week | 1 dev | 2,000 | **P1** |
| Phase 4: Game Adapter | 2 weeks | 1 dev | 4,000 | **P1** |
| Phase 5: AI Adapter | 1 week | 1 dev | 3,000 | **P1** |
| Phase 6: Cloud/IoT | 2 weeks | 1 dev | 3,000 | **P2** |
| Phase 7: Polish | 2 weeks | 1-2 devs | 2,000 | **P2** |
| **TOTAL** | **11 weeks** | **1-2 devs** | **22,000** | - |

**Với code reuse từ `/web/mg/` (40k lines):**
- Actual new code: ~22k lines
- Refactored code: ~18k lines
- **Total: ~40k lines** (giữ nguyên complexity, nhưng better organized)

---

## 17. CONCLUSION

### Summary
- ✅ Research-backed architecture (10 sources cited)
- ✅ Rust-first approach (industry standard 2026)
- ✅ 80% code reuse across cores
- ✅ Clean separation: core → adapters → CLI
- ✅ Preserves existing work (`_archive/`)
- ✅ Scalable for future cores

### Risk Mitigation
- 🛡️ Archive strategy: No data loss
- 🛡️ Incremental migration: Test at each step
- 🛡️ CI/CD from day 1: Catch regressions early
- 🛡️ Documentation-first: Clear implementation guide

---

**Ready for user approval and implementation! 🚀**
