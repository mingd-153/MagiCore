#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"

"$LAB_DIR/bootstrap.sh"
"$LAB_DIR/run-read-layer.sh"
"$LAB_DIR/run-tests.sh"
"$LAB_DIR/run-benchmarks.sh" "${1:-quick}"
"$LAB_DIR/run-security.sh"
"$LAB_DIR/compare-pms.sh"

echo "core-web platform lab run complete"
