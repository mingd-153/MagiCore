<div align="center">
  <img src="assets/logo-full.svg" alt="MagiCore Logo" width="full" />
  <h1>MagiCore</h1>
  <p><strong>Magical Core Management for the AI-Agent Era</strong></p>
  <p>
    <a href="https://github.com/mingd-153/MagiCore/releases"><img src="https://img.shields.io/github/v/release/mingd-153/MagiCore?label=latest&style=flat-square" alt="Latest Release" /></a>
    <a href="https://github.com/mingd-153/MagiCore/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/mingd-153/MagiCore/ci.yml?branch=main&label=CI&style=flat-square" alt="CI Status" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT License" /></a>
    <img src="https://img.shields.io/badge/rust-1.85%2B-orange?style=flat-square" alt="Rust 1.85+" />
    <img src="https://img.shields.io/badge/MCP-native-blueviolet?style=flat-square" alt="Native MCP Server" />
    <img src="https://img.shields.io/badge/version-1.0.0-brightgreen?style=flat-square" alt="Version 1.0.0" />
  </p>
</div>

---

**MagiCore** (`mgc`) is a single, fast, opinionated package manager written in Rust that handles **Web (Node.js/NPM), AI frameworks, Cloud IaC, CI/CD pipelines, Game engines, IoT toolchains, Mobile (Flutter/Swift/Kotlin), and Polyglot Libraries** — all with one consistent CLI.

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
| 🤖 **Native MCP Server**     | `mgc mcp` — built-in JSON-RPC 2.0 stdio server for AI IDEs (Cursor, Claude Code) |
| 🩺 **Smart Doctor**          | `mgc doctor --fix` auto-diagnoses and repairs environment issues                 |
| 🗄️ **Embedded Registry**     | `mgc-registry-server` — host your own private package registry                   |
| 🌍 **Cross-Platform**        | macOS (Apple Silicon + Intel), Linux x64/ARM64, Windows x64/ARM64               |

---

## 📦 Installation

### macOS (Homebrew)
```bash
brew install mingd-153/tap/magicore
```

### Windows (Scoop)
```powershell
scoop bucket add magicore https://github.com/mingd-153/scoop-magicore
scoop install magicore
```

### Download Binary (All Platforms)

Download the latest release from [**GitHub Releases →**](https://github.com/mingd-153/MagiCore/releases/latest)

| Platform            | File                          |
| ------------------- | ----------------------------- |
| macOS Apple Silicon | `magicore-macOS-ARM64.tar.gz` |
| macOS Intel         | `magicore-macOS-X64.tar.gz`   |
| Linux x64           | `magicore-Linux-X64.tar.gz`   |
| Linux ARM64         | `magicore-Linux-ARM64.tar.gz` |
| Windows x64         | `magicore-Windows-X64.zip`    |

```bash
# macOS/Linux
tar xzf magicore-*.tar.gz
sudo mv mgc /usr/local/bin/
mgc --version
```

### Build from Source
```bash
git clone https://github.com/mingd-153/MagiCore.git
cd MagiCore
cargo build --release --bin mgc
# Binary at: target/release/mgc
```
> **Requires:** Rust 1.85+

---

## 🚀 Quick Start

```bash
# Create a new web project
mgc create-web react@latest my-app --ts

# Install dependencies (auto-detects ecosystem)
mgc install

# Add a package
mgc add zod
mgc add -D vitest

# Run development server
mgc dev

# Security audit
mgc audit

# Generate SBOM (NEW in V1.0.0!)
mgc sbom --format cyclonedx-json --output sbom.json

# Check environment health
mgc doctor
```

### Monorepo / Workspace
```bash
# Install all packages across the entire monorepo
mgc install --recursive

# Run build in all workspaces
mgc build --recursive

# Filter specific packages
mgc build --recursive --filter "packages/*"
```

---

## 🤖 AI Coding Agent Setup (MCP)

MagiCore ships a **native Model Context Protocol server** — no Python runtime needed.

Add to your AI IDE config:

**Cursor** (`~/.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "magicore": {
      "command": "mgc",
      "args": ["mcp"]
    }
  }
}
```

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "magicore": {
      "command": "mgc",
      "args": ["mcp"]
    }
  }
}
```

MCP tools exposed: `mgc_install`, `mgc_add`, `mgc_audit`, `mgc_workspace_info`

---

## 🗂️ Project Structure

```
MagiCore/
├── cli/                    # mgc binary — CLI commands and dispatch engine
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
USAGE: mgc [OPTIONS] <COMMAND>

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
  mgc --help       Full command reference
  mgc <cmd> --help Per-command help
```

---

## 🔒 Security

**V1.0.0 Security Status**: ✅ Approved for CLI usage (see [SECURITY_AUDIT_V1.0.0.md](SECURITY_AUDIT_V1.0.0.md))

### Security Features
- ✅ **Cryptographically signed lockfiles** (Ed25519) for tamper detection
- ✅ **SRI (Subresource Integrity)** checksums for all packages
- ✅ **24-hour release quarantine** — newly published packages are flagged
- ✅ **SBOM generation** — CycloneDX & SPDX for supply chain visibility
- ✅ `mgc audit` scans for known CVEs via the advisory database
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
- ❌ `mgc why` command (dependency explanation)
- ❌ Lockfile version compatibility checks
- ❌ ~80% of test suite (requires v2 schema rewrite)

These features will be **restored in V1.0.1 hotfix (Week 7)** — estimated 1 week from V1.0.0 release.

**Workarounds**:
- Workspace projects: Each package maintains its own lockfile (no root merge)
- Install optimization: Full resolution on every install (slower but correct)
- Dependency explanation: Manual inspection of `mgc.lock`

See [docs/specs/magiCoreChangeLog.md](docs/specs/magiCoreChangeLog.md) for migration details.

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development environment setup
- Branching strategy & PR workflow
- Code style guidelines
- How to add a new ecosystem adapter

---

## 📄 License

MIT © MagiCore Contributors — see [LICENSE](LICENSE)
