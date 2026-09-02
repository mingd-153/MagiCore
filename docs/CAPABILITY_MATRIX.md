# MagiCore Capability Matrix (v1.1.0-RC)

**Last Updated**: 2026-09-02  
**Status**: RC (Release Candidate) — Public Beta

This matrix provides an honest assessment of MagiCore's capabilities across all cores. Use this to understand what works, what's experimental, and what's planned.

## Legend

| Status | Meaning |
|--------|---------|
| ✅ **Working** | Production-ready, tested, documented |
| 🧪 **Experimental** | Implemented but needs more testing/refinement |
| 🚧 **Beta-Blocked** | Scaffold exists, runtime pending MagiCore features |
| ⏳ **P2 Feature** | Planned for post-v1.1.0 release cycle |
| ❌ **Unsupported** | Not implemented, no current timeline |

---

## Core: `web`

| Capability | Status | Notes |
|------------|--------|-------|
| **Scaffolding** | ✅ Working | TypeScript/JavaScript project templates |
| **Package Management** | ✅ Working | npm/pnpm/yarn/bun support |
| **Dev Server** | ✅ Working | Vite/Next.js/Remix/SvelteKit detection |
| **Build** | ✅ Working | Production builds via detected bundler |
| **Optimizer** | ✅ Working | Runtime detection (Node/Bun/Deno), env loading |
| **Testing** | ✅ Working | Vitest/Jest/Playwright integration |
| **Security** | ✅ Working | Path traversal + shell injection protection |
| **E2E Verification** | ✅ Working | Bun env consumption tests passing |

**Overall**: ✅ Production-ready

---

## Core: `ai`

| Capability | Status | Notes |
|------------|--------|-------|
| **Scaffolding** | ✅ Working | Python ML/DL project templates (PyTorch/TensorFlow) |
| **Package Management** | ✅ Working | pip/poetry/uv support |
| **Dev Server** | 🧪 Experimental | Jupyter/FastAPI detection works, needs refinement |
| **Build** | 🧪 Experimental | Python wheel builds, Docker image support |
| **Optimizer** | ✅ Working | PyTorch/TensorFlow env detection, CUDA config |
| **Testing** | ✅ Working | pytest integration |
| **Security** | ✅ Working | Path traversal + shell injection protection |
| **E2E Verification** | ✅ Working | Python env consumption tests passing |

**Overall**: ✅ Working (dev/build need more real-world testing)

---

## Core: `app`

| Capability | Status | Notes |
|------------|--------|-------|
| **Scaffolding** | ✅ Working | Flutter/Kotlin/Swift/Multi templates |
| **Package Management** | ✅ Working | Flutter pub, Swift Package Manager, Gradle |
| **Dev Server** | 🧪 Experimental | Flutter works, iOS/Android needs testing |
| **Build** | 🧪 Experimental | Flutter builds work, native platform builds WIP |
| **Optimizer** | ✅ Working | Flutter/Kotlin/Swift env detection |
| **Testing** | 🧪 Experimental | Flutter test integration, native testing WIP |
| **Security** | ✅ Working | Path traversal + shell injection protection |
| **E2E Verification** | ✅ Working | Flutter env consumption tests passing |
| **React Native** | 🚧 Beta-Blocked | Scaffold exists, dev/build blocked until MagiCore-native app runner available |
| **CocoaPods Support** | ⏳ P2 Feature | iOS package management (post-v1.1.0) |
| **Maven Support** | ⏳ P2 Feature | Android package management (post-v1.1.0) |

**Overall**: 🧪 Experimental (Flutter solid, native platforms need testing)

---

## Core: `lib`

| Capability | Status | Notes |
|------------|--------|-------|
| **Scaffolding** | ✅ Working | Rust crate, Python package, TypeScript lib templates |
| **Package Management** | ✅ Working | Cargo, pip, npm integration |
| **Dev Server** | ✅ Working | cargo watch, pytest --watch support |
| **Build** | ✅ Working | Cargo build, Python wheel, tsc compilation |
| **Optimizer** | ✅ Working | Rust/Python/TypeScript env detection |
| **Testing** | ✅ Working | cargo test, pytest, vitest integration |
| **Security** | ✅ Working | Path traversal + shell injection protection |
| **E2E Verification** | ✅ Working | Python/TypeScript build env consumption tests passing |
| **pub.dev Support** | ⏳ P2 Feature | Dart package publishing (post-v1.1.0) |

**Overall**: ✅ Production-ready

---

## Core: `game` + `iot`

| Capability | Status | Notes |
|------------|--------|-------|
| **Scaffolding** | ✅ Working | Godot/Unity/Bevy, ESP32/PlatformIO templates |
| **Package Management** | 🧪 Experimental | Cargo (Bevy), PlatformIO lib support |
| **Dev Server** | 🧪 Experimental | Godot editor launch, PlatformIO monitor |
| **Build** | ✅ Working | Rust (Bevy) builds, PlatformIO firmware builds |
| **Optimizer** | ✅ Working | Rust/C++ env detection for game/iot |
| **Testing** | 🧪 Experimental | cargo test (Bevy), PlatformIO test support |
| **Security** | ✅ Working | Path traversal + shell injection protection |
| **E2E Verification** | ✅ Working | Rust env consumption verified (via BLOCKER 1) |
| **Godot/Unity Support** | 🧪 Experimental | Scaffold exists, engine-specific builds need testing |

**Overall**: 🧪 Experimental (Rust-based solid, proprietary engines need work)

---

## Cross-Cutting Capabilities

| Capability | Status | Notes |
|------------|--------|-------|
| **Multi-Core Projects** | ✅ Working | Detects multiple cores in single project |
| **Optimizer System** | ✅ Working | Runtime detection, env loading, process-level verification |
| **Security Hardening** | ✅ Working | cwd lock, shell injection prevention, path traversal protection |
| **CLI UX** | ✅ Working | Colored output, progress indicators, dry-run mode |
| **Package Distribution** | 🧪 Experimental | Homebrew formula ready (macOS ARM64 SHA verified), Scoop pending CI |
| **Documentation** | ✅ Working | README, SECURITY.md, packaging/README.md, this matrix |
| **Test Coverage** | ✅ Working | 14+ E2E tests (optimizer, security, lifecycle) all passing |

---

## Known Limitations (V1.1.0-RC)

### Blocked in Beta

- ❌ **React Native dev/build**: Scaffold exists, runtime blocked until MagiCore-native app runner available
  - Current behavior: Clear error message
  - Timeline: When MagiCore-native app runner complete

### P2 Features (post-v1.1.0)

- ⏳ **Maven Central support**: Kotlin/Android package management
- ⏳ **CocoaPods support**: iOS/macOS package management
- ⏳ **pub.dev support**: Dart/Flutter package publishing
  - Current behavior: Clear error messages directing users to native PM tools
  - Timeline: Next release cycle (2-3 weeks estimated)

### Experimental Areas Needing Real-World Testing

- 🧪 **App dev server**: Flutter works, iOS/Android simulator launch needs testing
- 🧪 **App native builds**: Cross-platform build orchestration needs refinement
- 🧪 **Game engine integration**: Godot/Unity editor launch needs testing
- 🧪 **AI dev server**: Jupyter/FastAPI detection works, auto-launch needs refinement

---

## Test Verification Status

| Test Suite | Status | Coverage |
|------------|--------|----------|
| **optimizer_e2e.rs** | ✅ Passing (3 tests) | Rust process-level RUSTFLAGS verification |
| **test_runner_security.rs** | ✅ Passing (6 tests) | cwd lock, shell injection, path traversal |
| **optimizer_lifecycle_e2e.rs** | ✅ Passing (5 tests) | web/ai/app/lib env consumption (Bun/Python/Flutter/Node) |

**Total**: 14 E2E tests, 0 ignored, 0 failing

---

## Recommendations by Use Case

### ✅ Ready for Production Use

- **Web applications** (TypeScript/JavaScript, any framework)
- **Rust libraries** (cargo-based workflows)
- **Python libraries** (pip/poetry-based workflows)
- **TypeScript libraries** (npm-based workflows)

### 🧪 Ready for Early Adopters

- **AI/ML projects** (PyTorch/TensorFlow, Python-based)
- **Flutter apps** (mobile/desktop)
- **Bevy games** (Rust-based game engine)
- **ESP32 IoT projects** (PlatformIO-based)

### 🚧 Wait for Next Release

- **React Native apps** (blocked until app runner ready)
- **Kotlin/Android apps using Maven** (P2 feature)
- **iOS apps using CocoaPods** (P2 feature)
- **Godot/Unity games** (needs engine-specific testing)

---

## Commitment to Honesty

This matrix will be updated with each release. We prioritize **honest assessment** over marketing claims. If something doesn't work reliably, we document it clearly and provide workarounds or timelines.

**Feedback**: Found a capability mismatch? Open an issue with `[capability-matrix]` tag.
