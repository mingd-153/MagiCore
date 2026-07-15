#!/usr/bin/env bash
set -e

echo "============================================="
echo " MegaGate vs bun vs pnpm vs npm Benchmark    "
echo "============================================="

# Ensure hyperfine is installed
if ! command -v hyperfine &> /dev/null; then
    echo "hyperfine could not be found. Please install it with: brew install hyperfine"
    exit 1
fi

# Build MegaGate in release mode
echo "=> Building MegaGate (release)..."
cargo build --release --workspace

MG_BIN="$PWD/target/release/mg"

# Create a test project inside the workspace to avoid sandbox issues
TEST_DIR="$PWD/benchmark_test_workspace"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

cat <<EOF > package.json
{
  "name": "benchmark-test",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "next": "^14.1.0",
    "lodash": "^4.17.21",
    "zod": "^3.22.4"
  },
  "devDependencies": {
    "typescript": "^5.3.3"
  }
}
EOF

# Prep commands (clear caches and node_modules)
PREP_MG="rm -rf node_modules mg.lock .megagate"
PREP_BUN="bun pm cache rm && rm -rf node_modules bun.lockb"
PREP_PNPM="pnpm store prune && rm -rf node_modules pnpm-lock.yaml"
PREP_NPM="npm cache clean --force && rm -rf node_modules package-lock.json"

echo "=> Running Benchmark (Cold Cache)..."

hyperfine --warmup 1 --runs 3 \
    --prepare "$PREP_MG" \
    --prepare "$PREP_BUN" \
    --prepare "$PREP_PNPM" \
    --prepare "$PREP_NPM" \
    -n "MegaGate" "$MG_BIN install" \
    -n "Bun" "bun install" \
    -n "pnpm" "pnpm install" \
    -n "npm" "npm install"

echo "============================================="
echo " Benchmark completed. Review the results!    "
echo "============================================="
