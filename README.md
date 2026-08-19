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
  </p>
</div>

---

**MegaGate** (`mg`) is a single, fast, opinionated package manager written in Rust that handles **Web (Node.js/NPM), AI frameworks, Cloud IaC, CI/CD pipelines, Game engines, IoT toolchains, Mobile (Flutter/Swift/Kotlin), and Polyglot Libraries** — all with one consistent CLI.

> **Beta Note:** `v0.3.0-beta.1` is functional and actively used in internal testing. APIs may change before `v1.0`. Please open issues and share feedback!

---

## ✨ Features

| Feature                     | Description                                                                     |
| --------------------------- | ------------------------------------------------------------------------------- |
| 🌐 **9 Ecosystems**          | Web, AI, Cloud, CI/CD, Game, IoT, App, Lib, Hardware — one CLI                  |
| ⚡ **Zero-Buffer Streaming** | Chunks stream directly from network → disk, no full-payload RAM spike           |
| 🔒 **Supply-Chain Security** | 24-hour new-release quarantine + SRI integrity verification                     |
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
sudo mv mg /usr/local/bin/
mg --version
```

### Build from Source
```bash
git clone https://github.com/mingd-153/MegaGate.git
cd MegaGate
cargo build --release --bin mg
# Binary at: target/release/mg
```
> **Requires:** Rust 1.85+

---

## 🚀 Quick Start

```bash
# Create a new web project
mg create-web my-app --framework react

# Install dependencies (auto-detects ecosystem)
mg install

# Add a package
mg add zod
mg add -D vitest

# Run development server
mg dev

# Security audit
mg audit

# Check environment health
mg doctor
```

### Monorepo / Workspace
```bash
# Install all packages across the entire monorepo
mg install --recursive

# Run build in all workspaces
mg build --recursive

# Filter specific packages
mg build --recursive --filter "packages/*"
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
      "command": "mg",
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
      "command": "mg",
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
USAGE: mg [OPTIONS] <COMMAND>

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

WORKSPACE COMMANDS:
  init            Create new project scaffold
  run             Execute a lifecycle script
  build           Build the project
  dev             Start local development server
  workspace       Manage monorepo workspaces

MORE:
  mg --help       Full command reference
  mg <cmd> --help Per-command help
```

---

## 🔒 Security

- All packages verified via **SRI (Subresource Integrity)** checksums.
- **24-hour release quarantine** — newly published packages are flagged.
- `mg audit` scans for known CVEs via the advisory database.
- Lifecycle scripts are **opt-in only** (trust gate).
- See [SECURITY.md](SECURITY.md) for vulnerability reporting.

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
