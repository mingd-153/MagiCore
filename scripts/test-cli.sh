#!/usr/bin/env bash
# MegaGate CLI integration tests
set -e

MGROOT="$(cd "$(dirname "$0")/.." && pwd)"
MG="$MGROOT/target/debug/mg"
PASS=0
FAIL=0
TMPDIR="/tmp/mg-test-$$"

cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

red()   { printf "\033[31m%s\033[0m\n" "$*"; }
green() { printf "\033[32m%s\033[0m\n" "$*"; }
bold()  { printf "\033[1m%s\033[0m\n" "$*"; }

pass() { PASS=$((PASS+1)); green "  v $1"; }
fail() { FAIL=$((FAIL+1)); red "  x $1"; }

run_ok() {
    local label="$1"; shift
    set +e
    local output
    output=$("$@" 2>&1)
    local rc=$?
    set -e
    if [ $rc -eq 0 ]; then
        pass "$label"
    else
        fail "$label (exit=$rc)"
        echo "$output" | tail -3
    fi
}

run_contains() {
    local label="$1" expect="$2"; shift 2
    set +e
    local output
    output=$("$@" 2>&1)
    local rc=$?
    set -e
    if echo "$output" | grep -Fq -e "$expect"; then
        pass "$label"
    else
        fail "$label (expected: $expect, exit=$rc)"
        echo "$output" | tail -3
    fi
}

# ─── 1. Build ─────────────────────────────────────────────────────

bold ""
bold "=== 1. Build ==="
cargo build -p mg 2>/dev/null
echo "  mg binary ready"

# ─── 2. Help ───────────────────────────────────────────────────────

bold ""
bold "=== 2. Help ==="
run_contains "shows mg command"       "MegaGate" "$MG" --help
run_contains "shows --core flag"      "--core"   "$MG" --help
run_contains "shows init"             "init"     "$MG" --help
run_contains "shows install"          "install"  "$MG" --help
run_contains "shows add"              "add"      "$MG" --help
run_contains "shows remove"           "remove"   "$MG" --help
run_contains "shows list"             "list"     "$MG" --help
run_contains "shows info"             "info"     "$MG" --help
run_contains "shows search"           "search"   "$MG" --help

# ─── 3. Registry ───────────────────────────────────────────────────

bold ""
bold "=== 3. Registry (live) ==="
run_contains "info lodash"    "Package: lodash" "$MG" info lodash
run_contains "search react"  "react@"          "$MG" search react

# ─── 4. Init ───────────────────────────────────────────────────────

bold ""
bold "=== 4. Init ==="
rm -rf "$TMPDIR" && mkdir -p "$TMPDIR"
run_ok "init --template web" "$MG" init --template web

# ─── 5. List + Add + Remove ────────────────────────────────────────

bold ""
bold "=== 5. Project flow ==="
mkdir -p "$TMPDIR/my-app"
cat > "$TMPDIR/my-app/package.json" << 'ENDJSON'
{"name":"my-app","version":"1.0.0","dependencies":{}}
ENDJSON

cd "$TMPDIR/my-app"
run_contains "list (empty)"    "No packages" "$MG" list
run_contains "add is-odd"      "Added"       "$MG" add is-odd
run_contains "list (after)"    "is-odd"      "$MG" list
run_contains "remove is-odd"   "Removed"     "$MG" remove is-odd

# ─── 6. Error cases ────────────────────────────────────────────────

bold ""
bold "=== 6. Errors ==="
cd "$TMPDIR"
mkdir -p empty-dir && cd empty-dir
run_contains "no project error" "No MegaGate" "$MG" list

cd "$TMPDIR/my-app"
run_contains "bad core error"   "not yet implemented" "$MG" --core game list

# ─── 8. Single-core build ─────────────────────────────────────────

bold ""
bold "=== 8. Single-core build ==="
cd "$TMPDIR"
mkdir -p solo && cd solo
cat > package.json << 'ENDJSON'
{"name":"solo","version":"1.0.0","dependencies":{}}
ENDJSON
MGSOLO="$MGROOT/target/debug/mg-solo"
test -f "$MGSOLO" || (
    cargo build -p mg --no-default-features --features web 2>/dev/null
    cp "$MGROOT/target/debug/mg" "$MGSOLO"
)
run_contains "solo: no --core needed" "No packages" "$MGSOLO" list

# ─── 9. Summary ────────────────────────────────────────────────────

bold ""
bold "────────────────────────────────"
bold "  $PASS passed / $FAIL failed"
bold "────────────────────────────────"

test $FAIL -gt 0 && exit 1 || exit 0
