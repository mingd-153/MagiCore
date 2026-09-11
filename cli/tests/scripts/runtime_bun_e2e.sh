#!/usr/bin/env bash
# Bun optimizer COMPAT-PROFILE env generation test (P0-4 2026-09-10)
# Status: CONFIG GENERATION ONLY (delegates to runtime_bun_e2e_impl.sh).
# The Bun profile requires compat mode (MGC_COMPAT_RUNTIME=bun) — the
# native lane never generates rival-runtime envs; bun binary never runs.

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec bash "$SCRIPT_DIR/runtime_bun_e2e_impl.sh"
