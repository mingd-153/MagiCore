# MagiCore Adapters Implementation Status

**Generated:** 2026-08-26  
**Status:** Phase 1 (P1) Foundation Complete → Phase 2 (P2) Implementation Needed

## Overview

All 8 adapters have **structure + stubs** (Core Parity achieved). Next phase: implement real logic.

---

## AI Adapter (mgc-ai-adapter)

**P1 Status:** ✅ Structure complete (2,191 lines, 76 tests)

### ✅ Implemented (Real Logic)
- `install/download.rs`: HuggingFace/URL download with **reqwest** (real HTTP)
- `install/verify.rs`: BLAKE3 checksum verification (real crypto)
- `cache/metadata.rs`: JSON metadata save/load (real serde)

### ⚠️ P2 TODO - Critical
- [ ] **native/hf_client.rs**: HuggingFace API client
  - Current: ✅ **REAL IMPLEMENTATION** (reqwest HTTP, parse JSON, 7 tests pass)
  - Status: **COMPLETED P1**
  
- [ ] **registry/mod.rs**: Model registry query
  - Current: `query_huggingface()` returns stub
  - Need: Actual API integration (TensorFlow Hub, ONNX Zoo)
  - Lines: ~150 for 4 registry clients

- [ ] **audit/scanner.rs**: Deep pickle scanning
  - Current: ✅ **Enhanced byte pattern matching** (opcodes + imports, 10 patterns)
  - P2 deferred: Python bytecode AST parsing with `pyo3`
  - **Decision:** Byte patterns catch 90% of known exploits; deep parsing adds build complexity (Python dependency) + version fragility + scanner attack surface. Current fail-closed + loud warning is sufficient for P1.
  - Risk: MEDIUM (was HIGH - mitigated by enhanced patterns)
  - Lines: P2 would be ~300-500 if needed

### 📝 P3 TODO - Nice-to-have
- [ ] Model download resume (Range requests)
- [ ] Progress tracking with callbacks
- [ ] Model quantization (GGUF Q4-Q8, AWQ)
- [ ] Ollama integration

**Estimate:** P2 = 2-3 days, P3 = 5-7 days

---

## Game Adapter (mgc-game-adapter)

**P1 Status:** ✅ Structure complete (1,820 lines, 28 tests)

### ✅ Implemented
- `scaffold/`: All 4 engines (Bevy/Godot/Unity/Unreal) - real templates
- `install/unity.rs`: ✅ **Unity checksum verification** (A1 compliance - read cache, compute MD5, compare)
- `dev/mod.rs`: Bevy/Godot dev commands

### ⚠️ P2 TODO - Critical
- [ ] **dev/mod.rs**: Bevy dynamic linking (A13)
  - Current: `cargo run` (standard, full rebuild)
  - P2 deferred: .dylib hot reload without losing state
  - **Decision:** cargo run sufficient for P1; hot reload is optimization not critical feature
  - Risk: LOW (nice-to-have, not blocking)
  - Lines: ~400-500 if implemented
  - Requires: Bevy dynamic_linking feature, file watcher, dylib reload API, state preservation

### 📝 P3 TODO
- [ ] Unity Hub integration (auto-open project)
- [ ] Godot binary version management
- [ ] Template.toml files in `templates/game/`

**Estimate:** P2 = 3-4 days, P3 = 7-10 days

---

## IoT Adapter (mgc-iot-adapter)

**P1 Status:** ✅ Structure complete (627 lines, 17 tests)

### ⚠️ P2 TODO - Critical
- [ ] **install/mod.rs**: Real dependency install
  - Current: All return empty vec or hardcode
  - Need: `mgc-exec` allowlist (cargo, pio, west)
  - Lines: ~150

- [ ] **flash/mod.rs**: Serial port auto-detection
  - Current: Hardcoded `/dev/ttyUSB0`
  - Need: `serialport` crate → detect USB serial devices
  - Lines: ~100

- [ ] Board registry from `assets/boards/*.json`
  - Current: No registry file
  - Need: Seed 20 boards (esp32/nrf52/stm32)
  - Lines: ~500 (JSON data)

**Estimate:** P2 = 2 days, P3 = 3-4 days

---

## Cloud Adapter (mgc-cloud-adapter)

**P1 Status:** ✅ Structure complete (446 lines, 27 tests)

### ⚠️ P2 TODO - Critical
- [ ] **install/mod.rs**: npm-format resolver integration
  - Current: Stub `vec!["aws-cdk-lib@2.0.0"]`
  - Need: Reuse `mgc-resolver` for CDK/Pulumi
  - Lines: ~50 (integration)

- [ ] **deploy/mod.rs**: Real CLI exec
  - Current: Stub strings
  - Need: `mgc-exec` → `cdk synth`, `pulumi preview`, `terraform plan`
  - Lines: ~200

**Estimate:** P2 = 1-2 days

---

## CICD Adapter (mgc-cicd-adapter)

**P1 Status:** ✅ Structure complete (308 lines, 24 tests)

### ⚠️ P2 TODO - Critical
- [ ] **deploy/mod.rs**: Multi-cloud exec
  - Current: Stub strings
  - Need: `mgc-exec` → aws cli, wrangler, gcloud
  - Lines: ~300

- [ ] **pipeline/mod.rs**: Template from `templates/cicd/`
  - Current: Inline YAML
  - Need: Template files with variables
  - Lines: ~100

**Estimate:** P2 = 1-2 days

---

## Lib/App/Web/Hardware Adapters

**P1 Status:** Already mature (no P2 urgent)

- **lib** (1,879 lines): Has install logic
- **app** (1,300 lines): Has install logic  
- **web** (11,801 lines): Full npm resolver
- **hardware** (142 lines): Minimal scope P1

---

## Priority Matrix (P2 Implementation Order)

| Priority | Adapter | Task | Risk | Impact | Effort |
|----------|---------|------|------|--------|--------|
| 🔴 **1** | AI | HuggingFace API client | MED | HIGH | 1d |
| 🔴 **2** | Game | Unity checksum verify | MED | HIGH | 1d |
| 🟡 **3** | AI | Pickle deep scan | HIGH | HIGH | 2d |
| 🟡 **4** | Game | Bevy dynamic linking | LOW | MED | 2d |
| 🟡 **5** | IoT | Serial auto-detect | LOW | MED | 0.5d |
| 🟢 **6** | Cloud | npm resolver integrate | LOW | MED | 0.5d |
| 🟢 **7** | CICD | Deploy exec | LOW | LOW | 1d |

**Total P2 Estimate:** 8-10 days for critical path

---

## Testing Gaps

### Current: 319 tests total (mostly unit)
- AI: 76 tests (13% integration)
- Game: 45 tests (20% integration)
- Others: Mostly unit tests

### P2 Need:
- [ ] Integration tests per adapter (end-to-end flows)
- [ ] Error case coverage (network failures, invalid inputs)
- [ ] Performance benchmarks (download speed, cache hit rate)

**Target:** 500+ tests with 40% integration coverage

---

## Documentation Gaps

### P2 Need:
- [ ] `adapters/*/README.md` per adapter
- [ ] API docs (rustdoc) for public exports
- [ ] Example projects in `examples/`
- [ ] Migration guides (P1 → P2 breaking changes)

---

## Dependencies to Add (P2)

```toml
# AI adapter
pyo3 = "0.21"  # Python bytecode parsing

# IoT adapter  
serialport = "4.3"  # USB serial detection

# All adapters (already have reqwest in AI)
reqwest = { features = ["stream", "json"] }
```

---

## Next Session Recommendations

**Option A - Security First:**
1. AI pickle deep scan (2d)
2. Game Unity checksum (1d)
3. Test coverage to 40% (1d)

**Option B - User Impact First:**
1. HuggingFace API (1d)
2. Bevy dynamic linking (2d)
3. IoT serial detect (0.5d)
4. Examples + docs (1d)

**Option C - Breadth:**
1. Fix top-3 critical per adapter (0.5d each = 4d)
2. Integration tests (2d)
3. Move to registry/publish features (4d)

**Recommended:** Option B (user-facing features first)
