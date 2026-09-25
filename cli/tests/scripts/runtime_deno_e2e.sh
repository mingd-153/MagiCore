#!/usr/bin/env bash
# Deno optimizer COMPAT-PROFILE env generation test (P0-4 2026-09-10)
# Status: CONFIG GENERATION ONLY (delegates to runtime_deno_e2e_impl.sh).
# The Deno profile requires compat mode (MGC_COMPAT_RUNTIME=deno) — the
# native lane never generates rival-runtime envs; deno binary never runs.

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec bash "$SCRIPT_DIR/runtime_deno_e2e_impl.sh"
