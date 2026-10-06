#!/usr/bin/env bash
# Deno optimizer COMPAT-PROFILE env generation test (P0-4 2026-09-10)
# Status: CONFIG GENERATION ONLY (delegates to runtime_deno_e2e_impl.sh).
# The Deno profile requires compat mode (MGC_COMPAT_RUNTIME=deno) — the
# native lane never generates rival-runtime envs; deno binary never runs.

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec /usr/bin/env -i PATH="$PATH" HOME="${HOME:-}" TMPDIR="${TMPDIR:-/tmp}" LANG="${LANG:-C.UTF-8}" CI="${CI:-}" GITHUB_ACTIONS="${GITHUB_ACTIONS:-}" GITHUB_WORKSPACE="${GITHUB_WORKSPACE:-}" MGC_BIN="${MGC_BIN:-}" MGC_BINARY="${MGC_BINARY:-}" MGC_CACHE_DIR="${MGC_CACHE_DIR:-}" PACKAGE_JSON="${PACKAGE_JSON:-}" MAGICORE_REPO_ROOT="${MAGICORE_REPO_ROOT:-}" TEST_HOME="${TEST_HOME:-}" bash "$SCRIPT_DIR/runtime_deno_e2e_impl.sh"
