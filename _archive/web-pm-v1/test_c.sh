#!/bin/bash
# One-command C test runner for mg-core-c
# Compiles and runs all C tests directly (bypasses Zig macOS linking issues)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CORE_C="$ROOT/crates/mg-core-c"
INCLUDE="$CORE_C/include"
SRC="$CORE_C/src"

echo "=== Compiling and running C tests ==="

for test_file in "$SRC/test/test_semver.c" "$SRC/test/test_json.c" "$SRC/test/test_sha256.c"; do
    test_name=$(basename "$test_file" .c)
    echo ""
    echo "--- $test_name ---"

    cc -o "/tmp/${test_name}" \
        "$SRC/semver.c" \
        "$SRC/json_extract.c" \
        "$SRC/sha256.c" \
        "$test_file" \
        -I"$INCLUDE" \
        -Wall -Wextra -Wpedantic \
        -Wno-unused-function

    "/tmp/${test_name}"
    rm -f "/tmp/${test_name}"
done

echo ""
echo "=== All C tests completed ==="
