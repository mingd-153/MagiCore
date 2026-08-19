# Contributing to MegaGate

Thank you for your interest in contributing! MegaGate is an open-source polyglot package manager for the AI-Agent era, and we welcome contributions of all kinds — bug reports, feature requests, documentation, and code.

---

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Project Structure](#project-structure)
- [Adding a New Ecosystem Adapter](#adding-a-new-ecosystem-adapter)
- [Testing Guidelines](#testing-guidelines)
- [Submitting a Pull Request](#submitting-a-pull-request)
- [Commit Message Format](#commit-message-format)

---

## Code of Conduct

We follow the [Contributor Covenant](https://www.contributor-covenant.org/). Be respectful, inclusive, and constructive.

---

## Getting Started

### Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | 1.85+ | [rustup.rs](https://rustup.rs/) |
| Node.js | 18+ | For web adapter tests |
| Git | 2.40+ | [git-scm.com](https://git-scm.com/) |
| cargo-deny | latest | `cargo install cargo-deny` |

### Setup

```bash
# 1. Fork and clone
git clone https://github.com/<your-fork>/MegaGate.git
cd MegaGate

# 2. Build the CLI in dev mode
cargo build --bin mg

# 3. Run the full test suite (should be 100% green)
cargo test --workspace

# 4. Verify the binary works
./target/debug/mg --version
```

---

## Development Workflow

### Branching Strategy

```
main               # Stable releases only (tagged)
  └── development  # Integration branch — all PRs target here
        └── feat-*       # New features
        └── fix-*        # Bug fixes
        └── docs-*       # Documentation only
        └── refactor-*   # Refactoring without behavior change
```

**All PRs must target `development`, NOT `main`.**

### Step-by-Step

```bash
# 1. Always start from development
git checkout development
git pull origin development

# 2. Create your branch
git checkout -b feat-your-feature-name

# 3. Make changes, test as you go
cargo test --workspace

# 4. Ensure no regressions
cargo check --workspace
cargo test --workspace

# 5. Commit (see commit format below)
git commit -m "feat(adapter): add XYZ support"

# 6. Push and open a PR against development
git push origin feat-your-feature-name
```

---

## Project Structure

```
MegaGate/
├── cli/src/
│   ├── commands/       # One file per CLI command (install.rs, audit.rs, mcp.rs…)
│   ├── dispatch/       # Command routing engine (common.rs, per_core.rs, engine.rs)
│   └── scaffold/       # Project scaffolding templates
│
├── core/crates/        # 18 foundational Rust crates
│   ├── mg-types/       # Shared types, IDs, error traits
│   ├── mg-store/       # Content-Addressable Storage (CAS)
│   ├── mg-resolver/    # Dependency graph solver (SAT)
│   ├── mg-fetcher/     # Streaming tarball download & extraction
│   ├── mg-lockfile/    # mg.lock read/write + 3-way merge
│   ├── mg-workspace/   # Monorepo topology + Catalogs protocol
│   ├── mg-platform/    # OS abstraction (reflink, fs_semaphore)
│   └── mg-http/        # Resilient HTTP client with retries
│
└── adapters/           # 9 ecosystem adapters
    ├── web/            # Node.js/NPM (most mature — reference implementation)
    ├── ai/             # AI/ML frameworks
    ├── cloud/          # IaC tools
    ├── cicd/           # CI/CD pipelines
    ├── game/           # Game engines
    ├── iot/            # Embedded / IoT
    ├── app/            # Mobile apps
    ├── lib/            # Polyglot libraries
    └── hardware/       # Hardware benchmarking
```

---

## Adding a New Ecosystem Adapter

If you want to add support for a new ecosystem (e.g., a new game engine or cloud platform):

1. **Create the adapter crate** under `adapters/<name>/` by copying `adapters/lib/` as a template.
2. **Implement the core traits** from `core/crates/mg-adapter-base/` (install, add, remove, list, audit).
3. **Wire the adapter** in `cli/src/commands/definitions.rs` (add new `Commands` variants) and `cli/src/dispatch/`.
4. **Write tests** under `adapters/<name>/tests/` with at minimum: install, add, scaffold, audit.
5. Open a PR describing what ecosystem is supported and link relevant documentation.

---

## Testing Guidelines

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p mg-fetcher

# Run a specific test
cargo test -p mg test_install_uses_cache

# Run with output (for debugging)
cargo test -p mg -- --nocapture
```

**Standards:**
- Every new command must have a CLI surface test in `cli/tests/`.
- Every new core crate function must have unit tests.
- Security-critical code (crypto, tarball extraction, store) requires both unit + integration tests.
- All tests must pass before a PR can be merged (`cargo test --workspace` exits 0).

---

## Submitting a Pull Request

1. Ensure **all tests pass**: `cargo test --workspace`
2. Ensure **no compilation errors**: `cargo check --workspace`
3. Fill in the [Pull Request Template](.github/PULL_REQUEST_TEMPLATE.md) completely.
4. Link any related issues with `Closes #NNN`.
5. Add a brief description of what changed and why.
6. A maintainer will review within a few business days.

**PR Checklist:**
- [ ] Tests added/updated for all new behavior
- [ ] `cargo test --workspace` passes locally
- [ ] `CHANGELOG.md` updated (if user-facing change)
- [ ] Docs updated (if adding a command or changing behavior)

---

## Commit Message Format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short description>

[optional body]
[optional footer(s)]
```

**Types:** `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `ci`

**Scopes:** crate name or area (`cli`, `fetcher`, `resolver`, `web`, `mcp`, `deploy`, etc.)

**Examples:**
```
feat(mcp): add mg_workspace_info tool to native MCP server
fix(fetcher): handle network timeout during streaming download
docs(readme): add MCP IDE integration guide
perf(platform): reduce APFS concurrency limit from 8 to 4 on macOS
test(web): add integration test for monorepo install with catalogs
```

---

## Reporting Bugs

Please open a [GitHub Issue](https://github.com/mingd-153/MegaGate/issues) with:
- MegaGate version (`mg --version`)
- OS and architecture
- Exact command that failed
- Full error output
- Steps to reproduce

---

## Questions?

- Open a [GitHub Discussion](https://github.com/mingd-153/MegaGate/discussions)
- Check [existing issues](https://github.com/mingd-153/MegaGate/issues)

Thank you for helping make MegaGate better! 🚀