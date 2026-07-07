# MegaGate - Visual Architecture Overview
**For**: Quick understanding of the new architecture  
**Date**: 2026-07-07

---

## 🎯 BIG PICTURE

```
┌─────────────────────────────────────────────────────────────┐
│                     MegaGate CLI                            │
│                   (mg command)                              │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│  mg init      │   │ mg create-*   │   │ mg install    │
│  (Interactive)│   │ (Scaffolding) │   │ (PM commands) │
└───────────────┘   └───────────────┘   └───────────────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            ▼
                    ┌───────────────┐
                    │ Adapter Layer │
                    │ (Auto-detect) │
                    └───────────────┘
                            │
        ┌───────────────────┼───────────────────┬───────────┐
        ▼                   ▼                   ▼           ▼
    ┌─────┐           ┌────────┐          ┌──────┐    ┌─────┐
    │ Web │           │  Game  │          │  AI  │    │ IoT │
    │ npm │           │Bevy/U5 │          │ PyPI │    │ ARM │
    └─────┘           └────────┘          └──────┘    └─────┘
        │                   │                   │           │
        └───────────────────┼───────────────────┴───────────┘
                            ▼
                    ┌───────────────┐
                    │  Shared Core  │
                    │ (80% reuse)   │
                    └───────────────┘
                            │
        ┌──────┬──────┬─────┼─────┬──────┬──────┐
        ▼      ▼      ▼     ▼     ▼      ▼      ▼
      HTTP  Store Crypto Lock Resolve Fetch  UI
```

---

## 📂 FOLDER STRUCTURE (Before vs After)

### ❌ BEFORE (Messy)
```
MegaGate/
├── web/
│   └── mg/                  # 40k lines Rust (isolated)
├── sdk/                     # Empty!
├── apps/                    # Empty!
├── packages/                # Empty!
├── bindings/                # Empty!
├── proto/                   # Empty!
├── templates/               # Empty!
└── CORE.md                  # Outdated docs
```

### ✅ AFTER (Clean)
```
MegaGate/
├── core/                    # 🦀 Shared Rust
│   └── crates/
│       ├── mg-http/         # HTTP client
│       ├── mg-store/        # CAS storage
│       ├── mg-crypto/       # SHA-256
│       ├── mg-lockfile/     # Lockfile format
│       ├── mg-resolver/     # PubGrub resolver
│       ├── mg-fetcher/      # Parallel downloads
│       ├── mg-ui/           # TUI (ratatui)
│       ├── mg-config/       # Config parser
│       └── mg-types/        # Shared types
│
├── adapters/                # 🔌 Ecosystem-specific
│   ├── web/                 # npm/pnpm/yarn
│   ├── game/                # Unity/Unreal/Bevy
│   ├── ai/                  # PyPI/conda
│   ├── cloud/               # Pulumi/Terraform
│   └── iot/                 # PlatformIO/Zephyr
│
├── cli/                     # 🎯 Unified binary
│   └── src/
│       ├── main.rs
│       └── commands/
│           ├── init.rs      # Interactive
│           ├── create.rs    # Scaffolding
│           ├── install.rs   # Package mgmt
│           └── ...
│
├── templates/               # 📁 Project templates
│   ├── web/
│   │   ├── vanilla/
│   │   ├── react-vite/
│   │   ├── next-app/
│   │   └── vue-vite/
│   ├── game/
│   │   ├── bevy/
│   │   ├── unity/
│   │   └── unreal/
│   ├── ai/
│   │   └── python-agent/
│   └── ...
│
└── _archive/                # 🗄 Preserved
    └── web-pm-v1/           # Original /web/mg/
        └── (40k lines kept for reference)
```

---

## 🔄 DATA FLOW

### User runs: `mg init`
```
1. CLI (cli/src/commands/init.rs)
   └─> Show interactive menu
       ├─> "What do you want to build?"
       ├─> User selects: "Web application"
       ├─> "Which framework?"
       └─> User selects: "React + Vite"

2. Template Processor
   └─> Copy templates/web/react-vite/
       └─> Replace {{name}}, {{author}}, etc.
           └─> Write to ./my-app/

3. Adapter Detection
   └─> Auto-detect: package.json → Web Adapter
       └─> Run: mg install (calls adapters/web/)

4. Web Adapter (adapters/web/src/lib.rs)
   └─> Parse package.json
       └─> Call Shared Core:
           ├─> mg-resolver: Resolve dependencies
           ├─> mg-fetcher: Download packages
           ├─> mg-store: Store in CAS
           └─> mg-installer: Link to node_modules/

5. Output
   └─> ✨ Created my-app! Run: cd my-app && mg dev
```

### User runs: `mg install react`
```
1. Adapter Detection (cli/src/detector.rs)
   └─> Found package.json → Load Web Adapter

2. Web Adapter (adapters/web/)
   ├─> Parse package.json
   ├─> Add "react": "^18.3.0"
   └─> Call resolve()

3. Shared Core (core/crates/)
   ├─> mg-resolver: PubGrub algorithm
   │   └─> Resolve dependency tree
   ├─> mg-fetcher: Download tarballs (parallel)
   │   └─> GET registry.npmjs.org/react/-/react-18.3.0.tgz
   ├─> mg-store: Extract to CAS
   │   └─> Store by SHA-256 hash
   └─> mg-installer: Hardlink to node_modules/
       └─> node_modules/react → ~/.mg/store/v1/sha256-abc123/

4. Output
   └─> ✓ Added react@18.3.0 (3.2s, 42 packages)
```

---

## 🎨 CLI UI EXAMPLES

### `mg init` Interactive
```
   ╭────────────────────────────────────────╮
   │  🚀 MegaGate - Universal PM            │
   ╰────────────────────────────────────────╯

   What do you want to build today?

     ❯ 🌐  Web application
       🎮  Game
       🤖  AI agent/tool
       ☁️   Cloud infrastructure
       🔌  IoT/Embedded device
       📦  Library/Package

   [↑↓] Navigate  [Enter] Select  [Esc] Cancel
```

### `mg install` Progress
```
📦 Installing dependencies...

react@18.3.0                 ████████████████████ 100%  ✓
react-dom@18.3.0             ████████████████████ 100%  ✓
vite@5.4.0                   ████████████████████ 100%  ✓
@types/react@18.3.0          ███████████████████▌  98%  ⏳

━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 342/342 packages

✓ Done in 3.2s (10x faster than npm)
💾 Saved 450 MB with deduplication
```

### `mg ui` TUI Dashboard
```
┌────────────────────────────────────────────────────┐
│ 📊 MegaGate Dashboard      [Tab] Switch  [q] Quit │
├────────────────────────────────────────────────────┤
│                                                    │
│  Project: my-app (React + Vite)                   │
│  Dependencies: 342 packages (89 dev)              │
│  Disk: 1.2 GB (450 MB saved by CAS)               │
│                                                    │
│  🔄 Recent Activity                                │
│  ├─ 2m ago: Added @types/node@20.10.0            │
│  ├─ 1h ago: Updated react 18.2.0 → 18.3.0       │
│  └─ Today: Installed 342 packages (3.2s)         │
│                                                    │
│  🛡️ Security: ✅ No vulnerabilities               │
│                                                    │
│  ⚡ Performance                                    │
│  ├─ Install: 3.2s (10x faster)                   │
│  ├─ Cache hit: 94%                                │
│  └─ Network: 12 MB ⬇️ 450 MB 💾                   │
└────────────────────────────────────────────────────┘
```

---

## 🔧 CODE REUSE STRATEGY

### What Gets Shared (80%)
```
┌──────────────────────────────────────────┐
│         Shared Core (Rust)               │
│  Used by ALL adapters (web/game/ai/...)  │
├──────────────────────────────────────────┤
│  ✓ HTTP client (reqwest wrapper)        │
│  ✓ Content-addressable store (CAS)      │
│  ✓ Integrity verification (SHA-256)     │
│  ✓ Lockfile format (unified JSON)       │
│  ✓ Dependency resolver (PubGrub)        │
│  ✓ Parallel fetcher (50 concurrent)     │
│  ✓ TUI components (progress, dashboard) │
│  ✓ Config management (TOML/YAML)        │
└──────────────────────────────────────────┘
```

### What's Adapter-Specific (20%)
```
┌─────────────┬─────────────┬─────────────┐
│ Web Adapter │Game Adapter │ AI Adapter  │
├─────────────┼─────────────┼─────────────┤
│ npm         │ Unity UPM   │ PyPI        │
│ registry    │ registry    │ registry    │
│             │             │             │
│ package.    │ manifest.   │ pyproject.  │
│ json parser │ json parser │ toml parser │
│             │             │             │
│ node_       │ Assets/     │ venv/       │
│ modules/    │ Plugins/    │ site-pkgs/  │
│ linker      │ linker      │ linker      │
└─────────────┴─────────────┴─────────────┘
```

---

## 🚀 IMPLEMENTATION TIMELINE

```
Week 1-2   ███████░░░░░░░░░░░  Foundation
           └─> Extract core/ from archive
           └─> Define adapter trait
           └─> CLI skeleton

Week 3     █████████████░░░░░  Web Adapter
           └─> Migrate npm logic
           └─> 811 tests passing

Week 4     ███████████████░░░  Templates
           └─> mg create-web works
           └─> Scaffolding system

Week 5-6   █████████████████░  Game Adapter
           └─> Bevy support
           └─> Unity/Unreal planning

Week 7     ███████████████████  AI Adapter
           └─> PyPI support
           └─> uv-compatible

Week 8+    ███████████████████  Polish
           └─> Cloud, IoT adapters
           └─> TUI dashboard
           └─> v1.0 release
```

---

## 📊 COMPARISON TABLE

| Feature | Old Structure | New Structure |
|---------|---------------|---------------|
| **Code organization** | ❌ Isolated `/web/mg/` | ✅ Shared `core/` |
| **Multi-ecosystem** | ❌ Web only | ✅ Web/Game/AI/Cloud/IoT |
| **Code reuse** | 0% | 80% |
| **CLI** | ❌ `mg install` only | ✅ `mg init`, `mg create-*` |
| **Templates** | ❌ None | ✅ 15+ templates |
| **Adapter system** | ❌ None | ✅ Pluggable adapters |
| **Documentation** | ⚠️ Outdated | ✅ Complete (600+ lines) |
| **Migration path** | N/A | ✅ Automated script |

---

## ✅ APPROVAL CHECKLIST

Để bắt đầu, bạn cần confirm:

- [ ] 📖 Đã đọc RESEARCH_REPORT.md
- [ ] 🏗️ Đã đọc ARCHITECTURE_PROPOSAL.md (17 sections)
- [ ] 📋 Đã đọc MIGRATION_SUMMARY.md
- [ ] 🎨 Đã xem VISUAL_OVERVIEW.md (file này)
- [ ] 🤔 Hiểu folder structure mới
- [ ] 🤔 Hiểu adapter pattern
- [ ] 🤔 Hiểu CLI commands (`mg init`, `mg create-*`)
- [ ] ⚠️ OK với việc move `/web/mg/` → `_archive/`
- [ ] ⚠️ OK với việc xóa folders trống (sdk/, apps/, etc.)
- [ ] ✅ Ready để run migration script

**Sau khi tick all boxes:**
```bash
# Run migration
./scripts/migrate-to-new-structure.sh

# Commit
git add -A
git commit -m "refactor: Migrate to multi-core architecture"
git push origin week-3
```

---

**Questions? Ask me anything! 🙂**
