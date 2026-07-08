# MegaGate CLI — Testing Guide

## Quick start

```bash
# Build
cargo build -p mg

# Show help
cargo run -- --help

# Query npm registry (no project needed)
cargo run -- info lodash
cargo run -- search react
```

## Project flow

```bash
# Create a test project
mkdir -p /tmp/test-app
cd /tmp/test-app

# Create package.json
cat > package.json << 'EOF'
{"name":"test-app","version":"1.0.0","dependencies":{}}
EOF

# List (empty)
cargo run -- list

# Add a dependency
cargo run -- add is-odd

# List (after add)
cargo run -- list

# Remove
cargo run -- remove is-odd

# Install all (resolve + fetch + link)
cargo run -- install
```

## Multi-core (full build)

```bash
# Specify --core flag
cargo run -- --core web list
cargo run -- --core web add react

# Try an unavailable core
cargo run -- --core game list
# → Error: Game core not available in this build
```

## Single-core build

```bash
# Build web-only binary
cargo build -p mg --no-default-features --features web

# Test (no --core needed)
./target/debug/mg list
./target/debug/mg add is-odd
```

## Interactive init

```bash
# Full build: shows 7-core selection menu
cargo run -- init

# Quick template (non-interactive)
cargo run -- init --template web
```

## Automated test suite

```bash
# Full build tests
./scripts/test-cli.sh

# Single-core tests
./scripts/test-cli.sh --single-core
```

## Scenarios covered

| # | Scenario | Command |
|---|---|---|
| 1 | Help shows all commands | `mg --help` |
| 2 | Registry info | `mg info lodash` |
| 3 | Registry search | `mg search react` |
| 4 | Init with template | `mg init --template web` |
| 5 | List empty project | `mg list` |
| 6 | Add dependency | `mg add is-odd` |
| 7 | List after add | `mg list` |
| 8 | Remove dependency | `mg remove is-odd` |
| 9 | No project → error | `mg list` (empty dir) |
| 10 | Core not available → error | `mg --core game list` |
| 11 | Single-core auto-default | `mg list` (no --core) |
| 12 | `--core` override | `mg --core web add react` |

## Architecture rules verified

- `.megagate/project.toml` ecosystem → wins (saved by `mg init`)
- `--core` flag → overrides `.megagate/` ecosystem
- `package.json` → detected only in CWD (no parent walk-up)
- `.megagate/` → detected in CWD + all parent directories
- Single-core build → auto-default to the only available core (no `--core` needed)
- Invalid core → clear error message with suggested brew install command
