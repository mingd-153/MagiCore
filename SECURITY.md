# Security Policy

MagiCore (`mgc`) takes security seriously. This document outlines our security features, vulnerability reporting process, and best practices.

---

## 🛡️ Security Features

### 1. **Supply-Chain Security (24-Hour Quarantine)**

MagiCore enforces a **24-hour new-release quarantine** by default to protect against malicious packages published in rapid succession (e.g., typosquatting, supply-chain attacks).

**How it works:**
- Packages published less than 24 hours ago are **blocked** during install
- Quarantine duration configurable per ecosystem via `mg.toml [security]`
- Escape hatch: `MAGICORE_ALLOW_UNTRUSTED=1` environment variable

**Configuration** (`mg.toml`):
```toml
[security]
min_release_age = 86400  # 24 hours (default)

# Per-ecosystem overrides
web = 86400   # 24 hours for JavaScript packages
ai = 259200   # 72 hours for Python AI packages (higher risk)
```

**Examples:**
- 3600 = 1 hour
- 86400 = 24 hours (default)
- 172800 = 48 hours
- 604800 = 1 week

See: [Gate 3 SecurityConfig](docs/examples/mg.toml.security)

---

### 2. **Trust Policy Management (Lifecycle Scripts)**

MagiCore implements a **trust gate** for lifecycle scripts (`install`, `postinstall`, etc.) to prevent malicious code execution.

**Default behavior:** All lifecycle scripts **blocked** until explicitly approved.

**Commands:**
```bash
# Approve a package to run lifecycle scripts
mgc trust approve lodash

# Deny a package (explicitly block)
mgc trust deny cowsay

# List all trust policies
mgc trust list

# Remove stale policies (packages no longer in project)
mgc trust prune
```

**Policy storage:** Policies stored in local database (`.magicore/cache/<core>/db.sqlite3`), project-specific.

**Enforcement:** Install fails if unapproved package attempts to run scripts. User must approve explicitly.

See: [Gate 2 Trust Commands](cli/src/commands/trust/)

---

### 3. **Cryptographically Signed Lockfiles**

MagiCore supports **Ed25519 signature verification** for `mgc.lock` to detect tampering.

**Commands:**
```bash
# Initialize keyring (generate Ed25519 keypair)
mgc trust init

# Sign lockfile
mgc trust sign mgc.lock

# Verify lockfile signature
mgc trust verify mgc.lock
```

**Signature format:**
```toml
[signature]
key_id = "ed25519:abc123..."
signature = "base64-encoded-signature"
signed_at = "2026-08-27T14:30:00Z"
```

**Verification:**
- `mgc install` automatically verifies signature if present
- Tampering detection: Signature mismatch → install fails
- Unsigned lockfiles: Warning printed, install proceeds (escape hatch)

---

### 4. **SBOM Generation (CycloneDX & SPDX)**

Generate Software Bill of Materials for compliance and vulnerability tracking.

**Commands:**
```bash
# Generate CycloneDX SBOM (JSON)
mgc sbom --format cyclonedx-json --output sbom.json

# Generate SPDX SBOM (JSON)
mgc sbom --format spdx-json --output sbom.spdx.json

# Generate CycloneDX XML
mgc sbom --format cyclonedx-xml --output sbom.xml
```

**Use cases:**
- Compliance audits (NIST, EU Cyber Resilience Act)
- Vulnerability scanning (integrate with Grype, Trivy)
- Dependency tracking for security reviews

**Note:** SBOM generation currently works for web core (npm packages). AI/app/lib support roadmap V1.0.1+.

---

### 5. **Audit Command (Vulnerability Scanning)**

Scan dependencies for known vulnerabilities.

**Commands:**
```bash
# Audit current project
mgc audit

# Audit with auto-fix (bump vulnerable packages)
mgc audit --fix

# Audit monorepo recursively
mgc audit --recursive
```

**Data sources:**
- **Web core:** npm audit API (native integration)
- **AI core:** OSV.dev (Python packages)
- **Other cores:** Delegates to ecosystem-native tools (cargo audit, etc.)

**Output:**
```
🛡️  MagiCore Security Audit (Web Core)
Audit Report
  Packages audited: 42
  Vulnerabilities: 2 (1 high, 1 medium)
  
  [HIGH] lodash < 4.17.21
  Path: lodash@4.17.20
  Fix: Upgrade to lodash@4.17.21
```

---

## 🚨 Reporting a Vulnerability

**DO NOT** open a public GitHub issue for security vulnerabilities.

### Reporting Process

1. **Email:** security@magicore.dev
2. **Subject:** `[SECURITY] <brief description>`
3. **Include:**
   - Description of the vulnerability
   - Steps to reproduce
   - Impact assessment (CVSS score if available)
   - Suggested fix (if any)

### Response Timeline

| Stage | Timeline |
|---|---|
| Initial acknowledgment | 48 hours |
| Triage and assessment | 7 days |
| Fix development | 14-30 days (depending on severity) |
| Coordinated disclosure | After fix released |

### Severity Levels

| Severity | Response Time | Examples |
|---|---|---|
| **Critical** | 24-48 hours | RCE, arbitrary code execution, supply-chain compromise |
| **High** | 7 days | Privilege escalation, authentication bypass |
| **Medium** | 14 days | Data leakage, denial of service |
| **Low** | 30 days | Information disclosure, minor bugs |

---

## 🔒 Security Best Practices

### For Users

1. **Enable quarantine:** Use default 24-hour quarantine (don't disable via `MAGICORE_ALLOW_UNTRUSTED=1` in production)
2. **Sign lockfiles:** Use `mgc trust init` + `mgc trust sign` to detect tampering
3. **Review trust policies:** Regularly audit `mgc trust list` and prune stale policies
4. **Run audits:** Include `mgc audit` in CI pipeline
5. **Generate SBOMs:** Track dependencies with `mgc sbom` for compliance
6. **Keep updated:** Run `mgc update` regularly to patch vulnerabilities

### For Contributors

1. **Follow RULE.md §3:** 5-step workflow (DEFINE → PLAN → BUILD → VERIFY → REVIEW, 2 loops)
2. **Security-sensitive code:** Add extra review round, invoke sub-agent for security audit
3. **Test coverage:** Write tests for security-critical paths (trust policies, quarantine, signature verification)
4. **No hardcoded secrets:** Use environment variables or secure config
5. **Input validation:** Sanitize all user input (package names, versions, paths)
6. **Fail-closed:** Default to secure behavior (deny, block, reject) with explicit escape hatches

---

## 📜 Supported Versions

| Version | Supported | Security Updates |
|---|---|---|
| 1.0.x | ✅ Yes | Active (latest stable) |
| 0.x.x | ❌ No | End of life |

**Upgrade policy:** Security patches released for latest stable version only. Users on older versions must upgrade to receive fixes.

---

## 🔗 Related Documentation

- [CONTRIBUTING.md](CONTRIBUTING.md) — Development workflow and testing guidelines
- [Gate 2: Trust Commands](cli/src/commands/trust/) — Trust policy implementation
- [Gate 3: SecurityConfig](docs/examples/mg.toml.security) — Quarantine configuration
- [Gate 4: Core Parity Matrix](docs/specs/CORE_PARITY_MATRIX_TEST.md) — Security feature availability per core
- [SECURITY_AUDIT_V1.0.0.md](SECURITY_AUDIT_V1.0.0.md) — V1.0.0 security audit report

---

## 📝 Security Changelog

### V1.0.0 (2026-08-27)

**Added:**
- 24-hour new-release quarantine (configurable via `mg.toml [security]`)
- Trust policy management (`mgc trust approve/deny/prune`)
- Cryptographic lockfile signatures (Ed25519)
- SBOM generation (CycloneDX, SPDX formats)
- Native audit command (`mgc audit` for web core)

**Fixed:**
- N/A (initial stable release)

---

## ✅ Security Audit Status

**Last audit:** 2026-08-27  
**Auditor:** Internal (MagiCore development team)  
**Scope:** V1.0.0 codebase (CLI, adapters, core crates)  
**Report:** [SECURITY_AUDIT_V1.0.0.md](SECURITY_AUDIT_V1.0.0.md)

**Findings:**
- **Critical:** 0
- **High:** 0
- **Medium:** 0 (all mitigated)
- **Low:** 2 (documented, acceptable risk)

**Next audit:** V1.1.0 (Q4 2026)

---

## 📧 Contact

- **Security issues:** security@magicore.dev
- **General questions:** support@magicore.dev
- **GitHub:** https://github.com/mingd-153/MagiCore/security

**PGP Key:** Available at https://magicore.dev/pgp-key.asc (optional)

---

*Last updated: 2026-08-27 | MagiCore V1.0.0*
