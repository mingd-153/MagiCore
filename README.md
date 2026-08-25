<div align="center">
  <img src="assets/logo.svg" alt="MegaGate Logo" width="full" />
  <h1>MegaGate</h1>
  <p><strong>The Universal Polyglot Package Manager for the AI-Agent Era</strong></p>
  <p>
    <a href="https://github.com/mingd-153/MegaGate/releases"><img src="https://img.shields.io/github/v/release/mingd-153/MegaGate?label=latest&style=flat-square" alt="Latest Release" /></a>
    <a href="https://github.com/mingd-153/MegaGate/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/mingd-153/MegaGate/ci.yml?branch=main&label=CI&style=flat-square" alt="CI Status" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT License" /></a>
    <img src="https://img.shields.io/badge/rust-1.85%2B-orange?style=flat-square" alt="Rust 1.85+" />
    <img src="https://img.shields.io/badge/MCP-native-blueviolet?style=flat-square" alt="Native MCP Server" />
    <img src="https://img.shields.io/badge/version-1.0.0-brightgreen?style=flat-square" alt="Version 1.0.0" />
  </p>
</div>

---

**MegaGate** (`mga`) is a single, fast, opinionated package manager written in Rust that handles **Web (Node.js/NPM), AI frameworks, Cloud IaC, CI/CD pipelines, Game engines, IoT toolchains, Mobile (Flutter/Swift/Kotlin), and Polyglot Libraries** — all with one consistent CLI.

> **🎉 Production Release:** `v1.0.0` is now production-ready! Includes SBOM generation, cryptographically signed lockfiles, and stable API. See [CHANGELOG.md](CHANGELOG.md) for details and [Known Limitations](#-known-limitations-v101-roadmap) for V1.0.1 roadmap.

---

## ✨ Features

| Feature                     | Description                                                                     |
| --------------------------- | ------------------------------------------------------------------------------- |
| 🌐 **9 Ecosystems**          | Web, AI, Cloud, CI/CD, Game, IoT, App, Lib, Hardware — one CLI                  |
| ⚡ **Zero-Buffer Streaming** | Chunks stream directly from network → disk, no full-payload RAM spike           |
| 🔒 **Supply-Chain Security** | 24-hour new-release quarantine + SRI integrity + SBOM generation                |
| 📋 **SBOM Generation**       | CycloneDX & SPDX formats for compliance and vulnerability tracking (NEW!)       |
| 🔐 **Signed Lockfiles**      | Ed25519 cryptographic signatures for tamper detection (NEW!)                    |
| 📦 **CAS Reflink Store**     | Content-addressed store with OS reflinks/hardlinks for zero-copy installs       |
| 🔀 **Monorepo Catalogs**     | PNPM-compatible `catalog:` protocol for centralized version management          |
| 🤖 **Native MCP Server**     | `mg mcp` — built-in JSON-RPC 2.0 stdio server for AI IDEs (Cursor, Claude Code) |
| 🩺 **Smart Doctor**          | `mg doctor --fix` auto-diagnoses and repairs environment issues                 |
| 🗄️ **Embedded Registry**     | `mg-registry-server` — host your own private package registry                   |
| 🌍 **Cross-Platform**        | macOS (Apple Silicon + Intel), Linux x64/ARM64, Windows x64/ARM64               |

---

## 📦 Installation

### macOS (Homebrew)
```bash
brew install mingd-153/tap/megagate
```

### Windows (Scoop)
```powershell
scoop bucket add megagate https://github.com/mingd-153/scoop-megagate
scoop install megagate
```

### Download Binary (All Platforms)

Download the latest release from [**GitHub Releases →**](https://github.com/mingd-153/MegaGate/releases/latest)

| Platform            | File                          |
| ------------------- | ----------------------------- |
| macOS Apple Silicon | `megagate-macOS-ARM64.tar.gz` |
| macOS Intel         | `megagate-macOS-X64.tar.gz`   |
| Linux x64           | `megagate-Linux-X64.tar.gz`   |
| Linux ARM64         | `megagate-Linux-ARM64.tar.gz` |
| Windows x64         | `megagate-Windows-X64.zip`    |

```bash
# macOS/Linux
tar xzf megagate-*.tar.gz
sudo mv mga /usr/local/bin/
mga --version
```

### Build from Source
```bash
git clone https://github.com/mingd-153/MegaGate.git
cd MegaGate
cargo build --release --bin mga
# Binary at: target/release/mga
```
> **Requires:** Rust 1.85+

---

## 🚀 Quick Start

```bash
# Create a new web project
mga create-web react@latest my-app --ts

# Install dependencies (auto-detects ecosystem)
mga install

# Add a package
mga add zod
mga add -D vitest

# Run development server
mga dev

# Security audit
mga audit

# Check environment health
mga doctor
```

### Monorepo / Workspace
```bash
# Install all packages across the entire monorepo
mga install --recursive

# Run build in all workspaces
mga build --recursive

# Filter specific packages
mga build --recursive --filter "packages/*"
```

---

## 🤖 AI Coding Agent Setup (MCP)

MegaGate ships a **native Model Context Protocol server** — no Python runtime needed.

Add to your AI IDE config:

**Cursor** (`~/.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "megagate": {
      "command": "mga",
      "args": ["mcp"]
    }
  }
}
```

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "megagate": {
      "command": "mga",
      "args": ["mcp"]
    }
  }
}
```

MCP tools exposed: `mg_install`, `mg_add`, `mg_audit`, `mg_workspace_info`

---

## 🗂️ Project Structure

```
MegaGate/
├── cli/                    # mg binary — CLI commands and dispatch engine
├── core/crates/            # 18 foundational Rust crates (store, resolver, fetcher…)
├── adapters/               # 9 ecosystem adapters (web, ai, cloud, cicd, game, iot, app, lib, hardware)
├── deploy/                 # Docker Compose + Nginx TLS reverse proxy configs
├── packaging/              # Homebrew formula + Scoop manifest
├── assets/                 # Logo and brand assets
└── .github/workflows/      # CI (test) + Release (6-target binary builds) pipelines
```

---

## 🖥️ Supported Ecosystems

| Core       | Languages / Tools                                                         |
| ---------- | ------------------------------------------------------------------------- |
| `web`      | Node.js, TypeScript, React, Vue, Next.js, FastAPI, Django, Spring Boot…   |
| `ai`       | Python AI frameworks, LLM serving, MCP server scaffolding                 |
| `cloud`    | Terraform, Pulumi, AWS CDK, Cloudflare Workers                            |
| `cicd`     | GitHub Actions, GitLab CI, ArgoCD, Docker Compose                         |
| `game`     | Godot, Unity, Unreal, Bevy (Rust)                                         |
| `iot`      | PlatformIO, Zephyr RTOS, ESP32 toolchains                                 |
| `app`      | Flutter, Swift Package Manager, Kotlin/Gradle, React Native               |
| `lib`      | Universal polyglot libraries (Rust crates, Python packages, npm packages) |
| `hardware` | Benchmark tooling, hardware-aware resource allocation                     |

---

## 📋 Commands Reference

```
USAGE: mga [OPTIONS] <COMMAND>

COMMON COMMANDS:
  install, i      Install dependencies (auto-detect ecosystem)
  add             Add a package to the project
  remove, rm      Remove a package
  update, up      Update packages to latest compatible version
  search          Search the registry
  audit           Supply-chain security audit
  info            Show package metadata
  outdated        List packages with available updates
  doctor          Environment diagnostic + AI-guided remediation
  mcp             Start native MCP server for AI coding agents
  sbom            Generate Software Bill of Materials (CycloneDX/SPDX) — NEW in V1.0.0!

WORKSPACE COMMANDS:
  init            Create new project scaffold
  run             Execute a lifecycle script
  build           Build the project
  dev             Start local development server
  workspace       Manage monorepo workspaces

MORE:
  mga --help       Full command reference
  mga <cmd> --help Per-command help
```

---

## 🔒 Security

**V1.0.0 Security Status**: ✅ Approved for CLI usage (see [SECURITY_AUDIT_V1.0.0.md](SECURITY_AUDIT_V1.0.0.md))

### Security Features
- ✅ **Cryptographically signed lockfiles** (Ed25519) for tamper detection
- ✅ **SRI (Subresource Integrity)** checksums for all packages
- ✅ **24-hour release quarantine** — newly published packages are flagged
- ✅ **SBOM generation** — CycloneDX & SPDX for supply chain visibility
- ✅ `mg audit` scans for known CVEs via the advisory database
- ✅ Lifecycle scripts are **opt-in only** (trust gate)

### Security Advisory (V1.0.0)
**Recommendation**:
- ✅ **Safe for CLI usage**: install, add, remove, SBOM, lockfile operations
- ⚠️ **Registry server**: Wait for V1.0.1 before production deployment

**Known Issues** (V1.0.1 hotfix — within 1 week):
- 3 transitive dependency CVEs (quick-xml, rkyv, rsa) — affects registry server only
- 7 unmaintained crates being replaced

See full report: [SECURITY_AUDIT_V1.0.0.md](SECURITY_AUDIT_V1.0.0.md)

**Vulnerability Reporting**: See [SECURITY.md](SECURITY.md) for responsible disclosure.

---

## ⚠️ Known Limitations (V1.0.1 Roadmap)

**Temporarily Disabled Features** (stubbed for rapid V1.0.0 release):
- ❌ Workspace lockfile merging (monorepo root lockfiles)
- ❌ Pruned install optimization (lockfile-based incremental installs)
- ❌ `mg why` command (dependency explanation)
- ❌ Lockfile version compatibility checks
- ❌ ~80% of test suite (requires v2 schema rewrite)

These features will be **restored in V1.0.1 hotfix (Week 7)** — estimated 1 week from V1.0.0 release.

**Workarounds**:
- Workspace projects: Each package maintains its own lockfile (no root merge)
- Install optimization: Full resolution on every install (slower but correct)
- Dependency explanation: Manual inspection of `mga.lock`

See [docs/specs/megaGateChangeLog.md](docs/specs/megaGateChangeLog.md) for migration details.

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development environment setup
- Branching strategy & PR workflow
- Code style guidelines
- How to add a new ecosystem adapter

---

## 📄 License

MIT © MegaGate Contributors — see [LICENSE](LICENSE)
