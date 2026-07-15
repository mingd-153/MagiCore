#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"
OUT="$LAB_DIR/benchmarks/pm-compare-$STAMP.md"
FIXTURES_DIR="$LAB_DIR/fixtures"

mkdir -p "$LAB_DIR/benchmarks"

describe_tool() {
  local tool="$1"
  if command -v "$tool" >/dev/null 2>&1; then
    echo "$(command -v "$tool")"
  else
    echo "missing"
  fi
}

latest_report_for() {
  local pm="$1"
  local fixture="$2"
  ls -t "$LAB_DIR/benchmarks/${pm}-${fixture}-"*.md 2>/dev/null | head -n 1 || true
}

extract_field() {
  local label="$1"
  local file="$2"
  grep -E "^- ${label}:" "$file" | head -n 1 | sed -E "s/^- ${label}: //" || true
}

extract_real_time() {
  local file="$1"
  grep -E '^real ' "$file" | tail -n 1 | awk '{print $2}' || true
}

{
  echo "# PM Comparison Lane"
  echo
  echo "- timestamp: $STAMP"
  echo
  echo "## Tool presence"
  echo
  for tool in pnpm bun npm hyperfine; do
    echo "- $tool: $(describe_tool "$tool")"
  done
  echo "- mg (global command name collision possible): $(describe_tool mg)"
  echo "- cargo: $(describe_tool cargo)"
  echo "- MegaGate lab runner: cargo run --bin mg --manifest-path cli/Cargo.toml --"
  echo
  echo "## Fixture inventory"
  echo
  for fixture in "$FIXTURES_DIR"/*; do
    [ -d "$fixture" ] || continue
    name="$(basename "$fixture")"
    file_count="$(find "$fixture" -type f | wc -l | tr -d ' ')"
    byte_count="$(find "$fixture" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
    echo "- $name: $file_count files, ${byte_count:-0} bytes"
  done
  echo
  echo "## Planned command matrix"
  echo
  echo "| Fixture | MegaGate | npm | pnpm | bun |"
  echo "|---|---|---|---|---|"
  echo "| react-vite-basic | install/add/dev smoke | install | install | install |"
  echo "| monorepo-basic | workspace install/layout | install | install | install |"
  echo "| monorepo-heavy | large workspace install/layout stress | install | install | install |"
  echo
  echo "## Current status"
  echo
  echo "- fixture baselines created locally"
  echo "- tool presence captured"
  echo "- next measurement step depends on actual PM binaries being installed locally"
  echo "- current smoke numbers are warm/local-state numbers, not normalized cold-start benchmarks"
  echo "- pnpm currently returns non-zero on react-vite-basic because of ignored build policy for esbuild, even though layout was materialized"
  echo "- monorepo-heavy makes the current workspace-behavior gap much easier to see across PMs"
  echo
  echo "## Latest smoke results"
  echo
  echo "| PM | Fixture | Exit | real(s) | node_modules files | node_modules bytes | Lockfile |"
  echo "|---|---|---:|---:|---:|---:|---|"
  for fixture_name in react-vite-basic monorepo-basic monorepo-heavy; do
    for pm in mg npm pnpm bun; do
      report="$(latest_report_for "$pm" "$fixture_name")"
      if [[ -n "$report" && -f "$report" ]]; then
        exit_status="$(extract_field 'exit-status' "$report")"
        real_time="$(extract_real_time "$report")"
        files="$(extract_field 'node_modules files' "$report")"
        bytes="$(extract_field 'node_modules bytes' "$report")"
        if [[ "$fixture_name" == monorepo-* ]]; then
          workspace_files="$(extract_field 'workspace node_modules files total' "$report")"
          workspace_bytes="$(extract_field 'workspace node_modules bytes total' "$report")"
          files="${workspace_files:-$files}"
          bytes="${workspace_bytes:-$bytes}"
        fi
        lock="none"
        if grep -q '^- mg.lock: yes' "$report"; then
          lock="mg.lock"
        elif grep -q '^- package-lock.json: yes' "$report"; then
          lock="package-lock.json"
        elif grep -q '^- pnpm-lock.yaml: yes' "$report"; then
          lock="pnpm-lock.yaml"
        elif grep -q '^- bun lock: yes' "$report"; then
          lock="bun.lock"
        fi
        echo "| $pm | $fixture_name | ${exit_status:-?} | ${real_time:-n/a} | ${files:-n/a} | ${bytes:-n/a} | $lock |"
      else
        echo "| $pm | $fixture_name | n/a | n/a | n/a | n/a | n/a |"
      fi
    done
  done
} > "$OUT"

echo "pm comparison scaffold: $OUT"
