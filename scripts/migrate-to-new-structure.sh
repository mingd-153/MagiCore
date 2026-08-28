#!/bin/bash
# MagiCore Migration Script
# Purpose: Migrate from old structure to new architecture
# Date: 2026-07-07
# DO NOT RUN without user confirmation!

set -e  # Exit on error

echo "🚀 MagiCore Structure Migration"
echo "================================"
echo ""
echo "⚠️  WARNING: This script will:"
echo "   1. Move /web/mgc/ → _archive/web-pm-v1/"
echo "   2. Delete empty folders (sdk/, apps/, packages/, etc.)"
echo "   3. Create new folder structure"
echo ""
read -p "Are you sure you want to continue? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    echo "❌ Migration cancelled"
    exit 1
fi

echo ""
echo "📦 Step 1: Archive /web/mgc/"
echo "----------------------------"
mkdir -p _archive
mv web/mgc _archive/web-pm-v1
cat > _archive/web-pm-v1/NOTE.md << 'EOF'
# Archived Web PM v1

This is the original MagiCore web package manager implementation (40k+ lines Rust).

**Status**: Archived for reference and code extraction.  
**Tests**: 811/811 passing  
**Date Archived**: 2026-07-07

## Purpose
Keep this code for:
- Reference during refactor
- Extracting reusable components to `core/`
- Ensuring no functionality is lost

## DO NOT
- Delete this folder
- Modify code here (read-only)
- Use in production (use new structure instead)

## Code Extraction Map
- `crates/mgc-store/` → `core/crates/mgc-store/`
- `crates/mgc-resolver/` → `core/crates/mgc-resolver/`
- `crates/mgc-lockfile/` → `core/crates/mgc-lockfile/`
- `crates/mgc-registry/` → `adapters/web/src/npm_registry.rs`
- `crates/mgc-fetcher/` → `core/crates/mgc-fetcher/`
- `crates/mgc-core/src/cffi/sha256.rs` → `core/crates/mgc-crypto/`
EOF

echo "✅ Archived to _archive/web-pm-v1/"
echo ""

echo "📦 Step 2: Remove empty/placeholder folders"
echo "--------------------------------------------"
rm -rf sdk/
rm -rf apps/
rm -rf packages/
rm -rf bindings/
rm -rf memanto/
rm -rf proto/
rm -rf examples/
rm -rf web/  # Now empty after moving mgc/
echo "✅ Removed empty folders"
echo ""

echo "📦 Step 3: Create new folder structure"
echo "---------------------------------------"

# Core
mkdir -p core/crates/{mgc-http,mgc-store,mgc-crypto,mgc-lockfile,mgc-resolver,mgc-fetcher,mgc-ui,mgc-config,mgc-types}

# Adapters
mkdir -p adapters/{web,game,ai,cloud,iot}/{src,tests}

# CLI
mkdir -p cli/src/commands

# Templates
mkdir -p templates/{web/{vanilla,react-vite,next-app,vue-vite,svelte},game/{bevy,unity,unreal,godot},ai/{python-agent,mcp-server},cloud/{pulumi-aws,terraform-gcp,cdk-typescript},iot/{esp32-rust,zephyr-arm},lib/{rust,typescript}}

# Docs
mkdir -p docs/{guides,adapters,api}

# Examples
mkdir -p examples

# Scripts (already exists, just ensure)
mkdir -p scripts

echo "✅ Created new folder structure"
echo ""

echo "📦 Step 4: Create workspace Cargo.toml files"
echo "----------------------------------------------"

# Root workspace
cat > Cargo.toml << 'EOF'
[workspace]
members = [
    "core/crates/*",
    "adapters/*",
    "cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/mingd-153/MagiCore"
authors = ["MagiCore Contributors"]

[workspace.dependencies]
# Async runtime
tokio = { version = "1.40", features = ["full"] }
async-trait = "0.1"

# HTTP
reqwest = { version = "0.12", features = ["json", "stream"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"

# CLI
clap = { version = "4.5", features = ["derive"] }
indicatif = "0.17"
console = "0.15"

# TUI
ratatui = "0.28"
crossterm = "0.28"

# Crypto
sha2 = "0.10"
hex = "0.4"

# Error handling
thiserror = "2.0"
anyhow = "1.0"

# Concurrency
dashmap = "6.1"
rayon = "1.10"

# Testing
proptest = "1.5"
rstest = "0.18"
EOF

# Core workspace
cat > core/Cargo.toml << 'EOF'
[workspace]
members = ["crates/*"]
resolver = "2"
EOF

echo "✅ Created Cargo.toml files"
echo ""

echo "📦 Step 5: Create README files"
echo "-------------------------------"

cat > core/README.md << 'EOF'
# MagiCore Core

Shared Rust components used across all adapters.

## Crates

- `mgc-http`: HTTP client wrapper
- `mgc-store`: Content-addressable store (CAS)
- `mgc-crypto`: Integrity verification (SHA-256)
- `mgc-lockfile`: Unified lockfile format
- `mgc-resolver`: Dependency resolver (PubGrub)
- `mgc-fetcher`: Parallel download manager
- `mgc-ui`: TUI components (ratatui)
- `mgc-config`: Configuration management
- `mgc-types`: Shared types, traits, errors

## Development

```bash
cd core
cargo test --all
```
EOF

cat > adapters/README.md << 'EOF'
# MagiCore Adapters

Ecosystem-specific package managers.

## Available Adapters

- `web/`: npm/pnpm compatibility
- `game/`: Unity, Unreal, Bevy, Godot
- `ai/`: PyPI, conda, HuggingFace models
- `cloud/`: Pulumi, Terraform, CDK
- `iot/`: Embedded Rust, PlatformIO, Zephyr

## Creating a New Adapter

See `docs/adapters/creating-adapter.md`
EOF

cat > templates/README.md << 'EOF'
# MagiCore Templates

Project scaffolding templates for `mgc create-*` commands.

## Template Variables

- `{{name}}`: Project name
- `{{author}}`: Author name
- `{{version}}`: Initial version
- `{{description}}`: Project description
- `{{license}}`: License (default: MIT)
- `{{year}}`: Current year

Files with `.tmpl` extension are processed with Handlebars.
EOF

echo "✅ Created README files"
echo ""

echo "✅ Migration Complete!"
echo "====================="
echo ""
echo "Next steps:"
echo "1. Review changes: git status"
echo "2. Extract code from _archive/web-pm-v1/ to core/"
echo "3. Build web adapter: cd adapters/web && cargo init"
echo "4. Create CLI: cd cli && cargo init"
echo ""
echo "See ARCHITECTURE_PROPOSAL.md for implementation phases."
