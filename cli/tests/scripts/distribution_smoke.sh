#!/usr/bin/env bash
# Distribution Smoke Test — delegates to real implementation
# Status: BASIC IMPLEMENTATION (calls distribution_smoke_impl.sh)

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec bash "$SCRIPT_DIR/distribution_smoke_impl.sh"
