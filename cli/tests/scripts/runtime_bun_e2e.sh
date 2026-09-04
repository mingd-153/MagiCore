#!/usr/bin/env bash
# Bun Runtime E2E Test — delegates to real implementation
# Status: IMPLEMENTED (calls runtime_bun_e2e_impl.sh)

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec bash "$SCRIPT_DIR/runtime_bun_e2e_impl.sh"
