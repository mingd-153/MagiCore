#!/usr/bin/env bash
# Dedupe benchmark — baseline (PreferLatest) vs --prefer-dedupe (02 §4)
set -euo pipefail

MG="${MG:-$(cd "$(dirname "$0")/../.." && pwd)/target/debug/mg}"
WORK="$(mktemp -d /tmp/mg-dedupe-bench.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

REACT_PIN='"react": "18.0.0", "react-dom": "18.0.0"'
LIBS='"react-router-dom": "^6.20.0", "@tanstack/react-query": "^5.0.0", "zustand": "^4.4.0", "@mui/material": "^5.14.0", "framer-motion": "^11.0.0", "react-dropzone": "^14.2.0"'

make_manifest() {
  local dir="$1" libs="$2"
  mkdir -p "$dir"
  cat > "$dir/package.json" <<EOF
{
  "name": "dedupe-bench",
  "version": "0.1.0",
  "private": true,
  "dependencies": {
    $REACT_PIN,
    $libs
  },
  "scripts": { "build": "echo bench" }
}
EOF
  cat > "$dir/mg.toml" <<'EOF'
name = "dedupe-bench"
version = "0.1.0"
ecosystem = "web"
EOF
}

count_instances() {
  # node_modules/.megagate/<name@version> dirs — strict layout
  find "$1/node_modules/.megagate" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l | tr -d ' '
}

vstore_size() {
  du -sk "$1/node_modules/.megagate" 2>/dev/null | awk '{print $1}'
}

# Seed: pin react 18.0.0 only, so the lockfile carries an older instance.
seed() {
  local dir="$1"
  mkdir -p "$dir"
  cat > "$dir/package.json" <<EOF
{
  "name": "dedupe-bench",
  "version": "0.1.0",
  "private": true,
  "dependencies": {
    $REACT_PIN
  },
  "scripts": { "build": "echo bench" }
}
EOF
  cat > "$dir/mg.toml" <<'EOF'
name = "dedupe-bench"
version = "0.1.0"
ecosystem = "web"
EOF
  (cd "$dir" && MEGAGATE_WEB_STRICT_LAYOUT=1 "$MG" install --ignore-scripts -q >/dev/null)
}

measure() {
  local dir="$1" label="$2"
  local start end secs
  start=$(python3 -c 'import time; print(time.time())')
  (cd "$dir" && MEGAGATE_WEB_STRICT_LAYOUT=1 "$MG" install --ignore-scripts -q ${3:-})
  end=$(python3 -c 'import time; print(time.time())')
  secs=$(python3 -c "print(round($end - $start, 2))")
  echo "$label install_secs=$secs instances=$(count_instances "$dir") vstore_kb=$(vstore_size "$dir")"
}

echo "== Dedupe benchmark =="
echo "mg: $MG"

seed "$WORK/base"
make_manifest "$WORK/base" "$LIBS"
measure "$WORK/base" "baseline(prefer-latest)"

seed "$WORK/dedupe"
make_manifest "$WORK/dedupe" "$LIBS"
measure "$WORK/dedupe" "prefer-dedupe" "--prefer-dedupe"

B_INS=$(count_instances "$WORK/base")
D_INS=$(count_instances "$WORK/dedupe")
echo "instances: baseline=$B_INS dedupe=$D_INS"
