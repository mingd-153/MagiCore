#!/usr/bin/env bash
# Bun optimizer environment test — delegates to implementation
# Status: ENV GENERATION ONLY (calls runtime_bun_e2e_impl.sh)

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec bash "$SCRIPT_DIR/runtime_bun_e2e_impl.sh"
