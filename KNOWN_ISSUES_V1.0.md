# Known Issues - MagiCore V1.0

**Last Updated**: 2026-08-27  
**Version**: 1.0.0-alpha

---

## 🔴 P0 - Critical Issues

### 1. vitest Dependency Crash

**Status**: 🔴 **UNRESOLVED** - Release Blocker  
**Severity**: Critical  
**Affects**: All users attempting to install vitest

#### Symptoms
```bash
mgc install-web  # with vitest in package.json
# Result: zsh: illegal hardware instruction
```

#### Root Cause
Unknown CPU-level fault during vitest dependency resolution. Binary search isolated to `vitest@^1.0.0` specifically.

#### Reproduction
```bash
cd /tmp/test
cat > package.json << 'EOF'
{
  "name": "test",
  "version": "1.0.0",
  "devDependencies": {
    "vitest": "^1.0.0"
  }
}
EOF
mgc install-web  # Crashes immediately
```

#### Workaround
**Option 1**: Use alternative test framework
```bash
# Instead of vitest, use:
mgc add -D jest @types/jest  # Jest
# or
mgc add -D @playwright/test  # Playwright
```

**Option 2**: Manual vitest installation (not recommended)
```bash
# Install other deps with mgc
mgc install-web
# Then use npm/pnpm for vitest only
npm install -D vitest
```

#### Impact
- ❌ Cannot use `mgc init --vitest` flag
- ❌ Any project with vitest in package.json will fail
- ❌ Benchmark comparison incomplete (19 vs 20 packages)
- ✅ All other test frameworks work (Jest, Playwright, etc.)

#### Fix Timeline
- **Target**: V1.1 (2-4 weeks)
- **Investigation**: In progress
- **Options**:
  1. Debug binary/CPU instruction issue
  2. Isolate vitest dependency tree conflict
  3. Update Rust toolchain/dependencies

#### Mitigation in V1.0
- ⚠️ `mgc init` will NOT include vitest by default
- ⚠️ README examples updated to use Jest
- ⚠️ Prominent warning in documentation
- ✅ Workarounds documented above

---

## ⚠️ P1 - High Priority Issues

### 2. Warm Install Speedup Lower Than Expected

**Status**: 🟡 **DOCUMENTED**  
**Severity**: Performance  
**Affects**: Users expecting faster re-installs

#### Expected vs Actual
- **Expected**: 30-50% speedup with cache
- **Actual**: ~2% speedup (1.63s cold → 1.59s warm)

#### Root Cause
- Resolver re-runs even with valid lockfile
- Cache helps download but not resolution phase
- Resolution dominates total time

#### Impact
- Cache works but benefit minimal
- G2 "peer cache" claim overstated

#### Fix Timeline
- **Target**: V1.1
- **Solution**: Skip resolver when lockfile valid + no manifest changes

---

### 3. Benchmark Methodology Limitations

**Status**: 🟡 **DOCUMENTED**  
**Severity**: Marketing claims  
**Affects**: Performance comparisons

#### Issues
| Issue | Impact |
|-------|--------|
| Different workload | mgc: 19 pkgs, competitors: 20 pkgs |
| Cold not isolated | Network/registry dependent |
| Bun wins warm | Bun: 0.28s, mgc: 1.59s |
| Raw data gitignored | Cannot independently verify |

#### Current Claims
✅ **Valid**: "Sub-2s installs in local testing (1.6s for 19-package workload)"  
❌ **Invalid**: "39x faster than pnpm" (methodology flaws)

#### Fix Timeline
- **Target**: V1.0.1 (this week)
- **Plan**:
  1. Same manifest for all PMs
  2. Separate cold/warm/offline/resolver/linker
  3. Run on macOS/Linux/Windows
  4. Commit raw data to tag
  5. Independent verification

---

### 4. G2 Cache Claim Inaccurate

**Status**: 🟡 **CORRECTION NEEDED**  
**Severity**: Documentation  
**Affects**: Technical accuracy

#### Claim vs Reality
- **Claim**: "Peer dependency cache improves warm install 30%"
- **Reality**: General dependency memoization, 2% actual speedup, in-memory only

#### Issues
- Not peer-specific
- Doesn't persist across CLI runs
- No A/B benchmark proof
- "70% cache hit" unsubstantiated

#### Fix Timeline
- **Target**: V1.0.1 (docs update)
- **Action**:
  1. Rename "peer cache" → "dependency memoization"
  2. Document actual 2% speedup
  3. Explain memory-only limitation
  4. Remove unproven claims

---

### 5. Security Workflow Gaps

**Status**: 🟡 **REQUIRES HARDENING**  
**Severity**: Supply chain  
**Affects**: Release security

#### Issues
```yaml
# .github/workflows/security.yml
osv-scanner -r . || true  # ❌ Vulnerabilities don't block CI
```

- 11 cargo advisories ignored without documentation
- GitHub Actions not SHA-pinned
- No artifact attestation/provenance

#### Fix Timeline
- **Target**: V1.0.1
- **Plan**:
  1. Remove OSV bypass
  2. Document ignored advisories with risk assessment
  3. Pin Actions to SHA
  4. Add SLSA provenance

---

## 📝 P2 - Medium Priority Issues

### 6. Code Quality Debt

**Status**: 🟢 **TRACKED**  
**Severity**: Maintainability  
**Affects**: Contributors

#### Issues
- 83 files with inline `#[cfg(test)] mod tests` (RULE violation)
- 92 TODO/FIXME/unimplemented/panic locations
- Some commands still `unimplemented!()`
- Lockfile validation temporarily disabled
- Offline flag incomplete

#### Fix Timeline
- **Target**: V1.1 (20 hours effort)
- **Tool**: `scripts/migrate_inline_tests.sh` ready

---

### 7. Multi-Core Depth Uneven

**Status**: 🟢 **BY DESIGN**  
**Severity**: Feature parity  
**Affects**: Non-web ecosystems

#### Current State
| Core | Depth | Status |
|------|-------|--------|
| Web | Native resolver/linker | ✅ Production |
| Lib-TS | Reuses web adapter | ✅ Production |
| Lib-Rust | Delegates to cargo | ⚠️ Orchestration |
| Lib-Python | Delegates to uv/pip | ⚠️ Orchestration |
| AI | Model download native | ⚠️ Mixed |
| App | Delegates to platform | ⚠️ Orchestration |

#### Not a Bug
- By design: web is native, others orchestration
- Multi-language support is real
- Depth will improve over time

#### Fix Timeline
- **Target**: V1.2+ (iterative improvement)

---

## 🎯 How to Report Issues

### Before Reporting
1. Check this document
2. Search [GitHub Issues](https://github.com/your-org/magicore/issues)
3. Try workarounds above

### Reporting Template
```markdown
**Issue**: Brief description
**Version**: mgc --version output
**OS**: macOS/Linux/Windows + version
**Reproduction**: Steps to reproduce
**Expected**: What should happen
**Actual**: What actually happened
**Logs**: Paste error output
```

### Priority Guidelines
- **P0 (Critical)**: Crashes, data loss, security
- **P1 (High)**: Major functionality broken
- **P2 (Medium)**: Minor bugs, performance
- **P3 (Low)**: Enhancements, docs

---

## 📊 Issue Tracking

| Issue | Priority | Status | Target | Owner |
|-------|----------|--------|--------|-------|
| vitest crash | P0 | Investigating | V1.1 | Core team |
| Warm speedup | P1 | Documented | V1.1 | Resolver |
| Benchmark method | P1 | Planning | V1.0.1 | Benchmarks |
| G2 cache claim | P1 | Doc fix | V1.0.1 | Docs |
| Security gaps | P1 | Planning | V1.0.1 | Security |
| Code quality | P2 | Tracked | V1.1 | Cleanup |
| Core parity | P2 | By design | V1.2+ | Architecture |

---

## ✅ What Works Well

Despite known issues, V1.0 has strong foundations:

✅ **Stable Features**:
- Next.js, React, TypeScript installation
- Content-addressable storage
- Lockfile V2 with signatures
- Trust system (approve/deny/prune)
- Multi-language detection & orchestration
- Supply chain controls

✅ **Performance**:
- Sub-2s installs for medium projects
- Consistent results (4.3% std dev)
- Competitive disk usage (380MB)

✅ **Security**:
- Fail-closed defaults
- Explicit lifecycle script approval
- Integrity verification
- BLAKE3/SRI checksums

---

**Conclusion**: V1.0 is usable for many workflows but has documented limitations. Choose vitest alternatives until V1.1.
