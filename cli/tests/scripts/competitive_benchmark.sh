#!/usr/bin/env bash
# Competitive Benchmark — delegates to real implementation
# Status: BASIC IMPLEMENTATION (calls competitive_benchmark_impl.sh)

set -euo pipefail

# Delegate to real implementation
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec bash "$SCRIPT_DIR/competitive_benchmark_impl.sh"
