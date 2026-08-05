# Contributing to MegaGate

Thank you for your interest in contributing to MegaGate! This document provides guidelines for contributing.

## Getting Started

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Node.js 18+ (for web adapter development)
- Git

### Setup

```bash
# Clone the repository
git clone https://github.com/your-org/MegaGate.git
cd MegaGate

# Build the CLI
cargo build --release

# Run tests
cargo test
```

## Development Workflow

### 1. Branching Strategy

- `main` - Stable releases
- `develop` - Integration branch for features
- Feature branches: `feature/description`
- Bugfix branches: `fix/description`
- Release branches: `release/vX.Y.Z`

### 2. Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): subject

body

footer
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`

Examples:
```
feat(core): add support for Go modules in game core
fix(web): resolve memory leak in dev server file watcher
docs(readme): update installation instructions
```

### 3. Pull Request Process

1. Create a feature branch from `develop`
2. Make your changes with tests
3. Ensure all tests pass: `cargo test`
4. Run linters: `cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings`
5. Submit PR to `develop` branch
6. Address review comments
7. Squash and merge after approval

## Code Style

### Rust

- Follow standard Rust style (rustfmt)
- Run `cargo fmt` before committing
- Run `cargo clippy --all-targets --all-features -- -D warnings` to catch issues
- Prefer `anyhow::Result` for error handling
- Use `tracing` for logging (not `println!`)

### Project Structure

```
MegaGate/
├── cli/                    # CLI binary
│   ├── src/
│   │   ├── commands/       # Command implementations
│   │   ├── dispatch/       # Command dispatch
│   │   ├── scaffold/       # Project scaffolding
│   │   └── wizard/         # Interactive project wizard
├── core/crates/            # Core library crates
│   ├── mg-types/           # Shared types & traits
│   ├── mg-config/          # Configuration
│   ├── mg-adapter-base/    # Base adapter implementation
│   ├── mg-resolver/        # Dependency resolution
│   ├── mg-lockfile/        # Lockfile handling
│   ├── mg-store/           # Content-addressable store
│   ├── mg-fetcher/         # Package fetching
│   ├── mg-http/            # HTTP client
│   └── mg-crypto/          # Cryptographic utilities
├── adapters/               # Ecosystem adapters
│   └── web/                # Web/Node.js adapter
└── templates/              # Scaffold templates
```

## Adding a New Core

1. Add core enum variant in `mg-types/src/ecosystem.rs`
2. Add adapter factory in `cli/src/factory.rs`
3. Create core command module in `cli/src/commands/core/`
4. Implement `PackageAdapter` trait in new adapter crate
5. Add scaffold templates in `templates/`
6. Add tests

## Testing

```bash
# Run all tests
cargo test

# Run specific package tests
cargo test -p mg
cargo test -p mg-web-adapter

# Run with specific test name
cargo test -p mg test_create_web_accepts_flags

# Run integration tests (requires npm)
cargo test --features integration-tests
```

## Code Quality Checks

```bash
# Format
cargo fmt --all -- --check

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Audit dependencies
cargo audit
```

## Documentation

- Update `README.md` for user-facing changes
- Update doc comments for public APIs
- Update `CLI_FULL_SURFACE_2026-07-16.md` for CLI changes

## Reporting Issues

- Use GitHub Issues
- Include reproduction steps
- Include `mg --version` and OS info
- For security issues, see [SECURITY.md](SECURITY.md)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.