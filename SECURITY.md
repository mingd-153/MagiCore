# MagiCore Security Model

**Version:** 1.1.0-RC
**Last Updated:** 2026-09-01

MagiCore implements a comprehensive security model designed to protect against supply-chain attacks, malicious packages, and credential theft. This document explains the security mechanisms available to users.

## Security Principles

1. **Fail-closed by default**: Security mechanisms default to blocking unsafe operations
2. **Explicit escape hatches**: Bypasses require explicit flags and generate audit logs
3. **Immutable allowlists**: Tool execution is restricted to a curated, reviewed list
4. **Zero-trust dependencies**: All packages are verified before installation

## 1. Execution Allowlist (exec passthrough)

MagiCore restricts which external tools can be executed during package operations.

### Allowed Tools

The following tools are permitted (core/crates/mgc-exec/src/allowlist.rs):

```
pip, python3, uv, go, pub, dart, gradle, mvn, composer, node, swift, cargo,
espflash, west, pio, platformio, terraform, tofu, cdk, pulumi, aws, wrangler,
gcloud, gh, git, docker, godot, flutter, kotlinc, python, unity, upm, xcodebuild
```

### Permanently Forbidden Tools

These package managers are **permanently forbidden** because MagiCore provides native resolution for their formats:

```
npm, npx, pnpm, yarn, bun, bunx
```

**Why?** MagiCore implements its own npm-format resolver. Allowing external package manager wrappers would:
- Bypass security controls (script execution, lockfile verification)
- Create supply-chain confusion
- Execute unaudited lifecycle scripts

**Correct usage:**
```bash
# ❌ FORBIDDEN
npm install react

# ✅ CORRECT
mgc install react
```

### Adding Tools to Allowlist

Adding tools requires security review. See `sys-mgc/00-index.md` §5 for process.

### Error Messages

```bash
# Tool not on allowlist
$ mgc exec custom-tool
Error: tool 'custom-tool' is not on the allowlist (00-index §5.1)

# Permanently forbidden tool
$ mgc exec npm install
Error: tool 'npm' is permanently forbidden (mgc resolver covers its format — use `mgc install` instead)
```

## 1.1. Test Runner Security Model

**Status:** Implemented in v1.1.0-RC (P0-1)

### Execution Scopes

MagiCore distinguishes between different execution contexts with different security postures:

| Scope | Package Managers Allowed? | Use Case |
|-------|---------------------------|----------|
| `Install` | ❌ Forbidden | Scaffold, install, dependency resolution |
| `TestRunner` | ✅ Allowed | Running project test suites (`mgc test`) |
| `BuildRunner` | ✅ Allowed | Building projects (`mgc build`) |
| `DevServer` | ✅ Allowed | Development servers (`mgc dev`) |

**Why different scopes?**

- **Install scope** = HIGH RISK: Runs before user sees project code, must use MagiCore's resolver
- **TestRunner scope** = MEDIUM RISK: Runs after user control, delegates to project's declared test runner

### Test Runner Behavior

When you run `mgc test`, the system:

1. **Auto-detects** project test runner from:
   - `package.json` `scripts.test` (Node.js)
   - `Cargo.toml` (Rust)
   - `pyproject.toml` or `pytest` (Python)
   - `go.mod` + `go test` (Go)
   - `pubspec.yaml` + `flutter test` (Flutter)

2. **Executes** the detected runner with `ExecutionScope::TestRunner`

3. **Logs** execution to audit trail (if enabled)

### Security Tests

Verification tests at `cli/tests/test_runner_security.rs`:

| Test | Status | Description |
|------|--------|-------------|
| `test_npm_allowed_in_test_runner_scope` | ✅ PASS | npm/pnpm/yarn allowed in TestRunner |
| `test_npm_forbidden_in_install_scope` | ✅ PASS | npm/pnpm/yarn rejected in Install |
| `test_pnpm_allowed_in_test_runner_scope` | ✅ PASS | Explicit runner selection works |
| `test_audit_log_records_execution` | ✅ PASS | Tool executions logged |
| `test_cwd_lock_prevents_traversal` | ⚠️ IGNORED | TODO: cwd lock not yet enforced |
| `test_shell_injection_prevented` | ⚠️ IGNORED | TODO: shell escaping not yet implemented |

**Known Gaps (P1 follow-up):**

1. **No cwd lock**: Test runners can `cd` outside project root
2. **Shell injection**: Scripts can use shell metacharacters to escape sandboxing
3. **No resource limits**: No CPU/memory/network constraints on test processes

These gaps are documented and tracked for P1 milestone.

### Manual Override

```bash
# Use specific test runner
$ mgc test --runner jest

# Disable test runner (fail if auto-detect fails)
$ mgc test --no-auto-detect
```

## 2. Script Execution Policy

Package lifecycle scripts (install, postinstall, etc.) are controlled by security policy.

### Script Modes

Configure in `mgc.toml`:

```toml
[security]
scripts.mode = "trusted"  # default: only run trusted packages
```

| Mode | Behavior |
|------|----------|
| `trusted` (default) | Run scripts only for packages in trust-list; warn for untrusted |
| `all` | Run all scripts (⚠️ npm compatibility mode — warns once) |
| `none` | Disable all scripts |
| `quarantine` | Run untrusted scripts in sandboxed environment (P1.5) |

### Trust List

Trust is based on `(package_name, integrity_hash)` pairs — not just names. This prevents typosquatting and dependency confusion.

```bash
# Add package to trust list
$ mgc trust add react --yes

# View trust list
$ mgc trust list

# Remove from trust list
$ mgc trust remove old-package
```

Trust entries are stored in `mgc.toml` and auditable.

### CLI Override

```bash
# Disable scripts for this install
$ mgc install --ignore-scripts

# Force-run scripts (not recommended)
$ mgc install --scripts=all
```

## 3. Credential Management

### Secure Token Storage

```bash
# Store token in OS keyring (recommended)
$ mgc login registry.example.com

# Logout and revoke token
$ mgc logout registry.example.com
```

Tokens are stored in:
1. **OS Keyring** (preferred): macOS Keychain, Windows Credential Manager, Linux libsecret
2. **Encrypted file** (fallback): `~/.magicore/credentials` (chmod 0600, encrypted)
3. **`.npmrc`** (legacy): Read for compatibility but warns about plaintext storage

### Token Masking

All credential values are **masked before logging**:
- Config dumps never show `_authToken` values
- Exec logs redact `--token`, `--password`, `--key` arguments
- Error messages never include credentials

## 4. Audit & Vulnerability Scanning

### Check for Known Vulnerabilities

```bash
# Scan dependencies for CVEs
$ mgc audit

# Air-gapped mode (download OSV database locally)
$ mgc audit --update-db  # run once
$ mgc audit --offline     # subsequent scans
```

**Privacy:** Default "online-safe" mode sends SHA256 hashes of package names — **not** actual package names or project structure.

### Quarantine Policy

Packages with known vulnerabilities are quarantined for 24 hours by default:

```toml
[security]
quarantine_hours = 24  # adjustable
```

Manual override (use with caution):

```bash
$ mgc audit --unblock package-name --yes
```

All unblock actions are logged in audit trail.

## 5. Supply Chain Protection

### Lockfile Verification

Lockfiles are cryptographically signed to detect tampering:

```bash
# Verify lockfile integrity
$ mgc verify

# Regenerate lockfile signature
$ mgc lock --sign
```

Lockfile signatures use BLAKE3 keyed hashing. Key stored in `~/.magicore/keys/lockfile.key` (chmod 600).

### Dependency Confusion Detection

MagiCore detects and blocks:
- **Typosquatting**: Package names similar to popular packages (Levenshtein distance ≤2)
- **Homoglyph attacks**: Confusable Unicode characters (`reаct` with Cyrillic 'а')
- **Scope confusion**: Private packages with same name as public packages

### SBOM Generation

```bash
# Generate Software Bill of Materials
$ mgc sbom                          # SPDX format
$ mgc sbom --format cyclonedx       # CycloneDX format

# Compare SBOM for CI review
$ mgc sbom --diff baseline.json
```

## 6. Audit Logging

All security-relevant operations are logged to `~/.magicore/exec.log`:

```bash
# View audit log
$ tail -f ~/.magicore/exec.log

# Verify audit log integrity (P2)
$ mgc audit log-verify
```

Log entries include:
- Command executed
- Arguments (credentials redacted)
- Working directory
- Exit code
- Duration
- Timestamp

## 7. Secure Defaults

MagiCore follows these secure-by-default practices:

| Feature | Default | Escape Hatch |
|---------|---------|--------------|
| Script execution | Trusted packages only | `--scripts=all` |
| TLS verification | Required | `MAGICORE_ALLOW_UNTRUSTED=1` (warns) |
| Exec allowlist | Enforced | Cannot bypass |
| Audit scanning | Enabled | `--no-audit` |
| 24h quarantine | Enabled | `--unblock` |
| Lockfile signing | Enabled | — |

## 8. Reporting Vulnerabilities

To report security issues in MagiCore:

1. **DO NOT** open public GitHub issues
2. Email: security@magicore.dev (coming soon)
3. Include:
   - MagiCore version (`mgc --version`)
   - Reproduction steps
   - Impact assessment

See `.github/SECURITY_EXCEPTIONS.toml` for known acceptable risks.

## 9. Security Roadmap

### P1 (Current — v1.1.0-RC)
- ✅ Exec allowlist with forbidden tools
- ✅ Script trust-list with integrity hashing
- ✅ Credential masking in logs
- ✅ Typosquat detection (Levenshtein + homoglyph)
- ✅ Lockfile signing (BLAKE3)
- ✅ Dependency confusion blocking
- ✅ 24h quarantine for vulnerable packages
- ✅ Privacy-safe audit (hash-based OSV queries)

### P1.5 (Next — pre-v1.2.0)
- [ ] System-level sandboxing (macOS sandbox-exec, Linux seccomp)
- [ ] AI/IoT core sandbox enforcement

### P2 (Future)
- [ ] TUF metadata signing for registry
- [ ] OIDC-based provenance (Sigstore)
- [ ] Immutable audit trail (hash chain)
- [ ] OS keyring support for all platforms

## 10. Configuration Reference

```toml
# mgc.toml
[security]
# Script execution policy
scripts.mode = "trusted"  # trusted | all | none | quarantine

# Trusted packages (added via `mgc trust add`)
scripts.trusted_packages = [
    { name = "react", integrity = "sha512-abc123..." }
]

# Quarantine settings
quarantine_hours = 24

# Audit mode
audit.mode = "online-safe"  # online-safe | air-gapped

# Lockfile signing
lockfile.sign = true
lockfile.key_path = "~/.magicore/keys/lockfile.key"

# Registry allowlist for non-scoped private packages (prevents confusion)
[registry.private_packages]
allowed = ["my-internal-lib", "company-sdk"]
```

## 11. Best Practices

1. **Use OS keyring for tokens**: Run `mgc login` instead of editing `.npmrc`
2. **Enable audit in CI**: Add `mgc audit` to your CI pipeline
3. **Review trust list**: Periodically audit `mgc trust list`
4. **Monitor audit log**: Set up alerts for unexpected tool executions
5. **Pin registries**: Use lockfile registry pinning to prevent MITM
6. **Rotate keys**: Run `mgc keys rotate` annually
7. **Air-gap for sensitive projects**: Use `mgc audit --offline` mode
8. **Never disable security**: Avoid `--ignore-scripts` and `MAGICORE_ALLOW_UNTRUSTED`

## 12. Security Architecture

```
User Command
    ↓
CLI Dispatch (cli/src/dispatch/)
    ↓
Security Gates:
    ├─ Allowlist Check (mgc-exec/allowlist.rs)  ← FAIL-CLOSED
    ├─ Script Policy (mgc-resolver/trust.rs)    ← TRUSTED-ONLY DEFAULT
    ├─ Credential Mask (mgc-crypto/)            ← REDACT BEFORE LOG
    ├─ Audit Check (cli/commands/audit.rs)      ← QUARANTINE 24H
    └─ Lockfile Verify (mgc-lockfile/)          ← SIGNATURE REQUIRED
    ↓
Adapter Execution (adapters/*/src/adapter.rs)
    ↓
Audit Log (~/.magicore/exec.log)
```

All security gates are **fail-closed**: operations blocked unless explicitly allowed.

---

**Questions?** See `sys-mgc/20-security-deep.md` for technical deep-dive or open a discussion on GitHub.
