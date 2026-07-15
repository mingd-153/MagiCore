#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
FIXTURES_DIR="$LAB_DIR/fixtures"
STAMP="$(date '+%Y%m%d-%H%M%S')"

FIXTURE="${1:-monorepo-heavy}"
SRC_DIR="$FIXTURES_DIR/$FIXTURE"
if [[ ! -d "$SRC_DIR" ]]; then
  echo "unknown fixture: $FIXTURE" >&2
  exit 1
fi

BASE_DIR="$(mktemp -d "/private/tmp/core-web-workspace-bench.${FIXTURE}.XXXXXX")"
SHARED_CACHE_DIR="$BASE_DIR/shared-cache"
REPORT="$LAB_DIR/benchmarks/workspace-bench-${FIXTURE}-${STAMP}.md"

mkdir -p "$SHARED_CACHE_DIR"

{
  echo "# Workspace Benchmark"
  echo
  echo "- timestamp: $STAMP"
  echo "- fixture: $FIXTURE"
  echo "- base-dir: $BASE_DIR"
  echo "- shared-cache-dir: $SHARED_CACHE_DIR"
  echo
  echo "## Scenarios"
  echo
  echo "| Scenario | Meaning |"
  echo "|---|---|"
  echo "| cold-project-empty-shared-cache | fresh project + empty dedicated shared cache |"
  echo "| warm-reinstall-same-project | second install in the exact same project |"
  echo "| fresh-project-warm-shared-cache | new fresh project after the shared cache has been populated |"
  echo
} > "$REPORT"

mg_cmd() {
  if [[ -x "$ROOT/target/debug/mg" ]]; then
    "$ROOT/target/debug/mg" install
  else
    cargo run --manifest-path "$ROOT/cli/Cargo.toml" --bin mg --no-default-features --features web -- install
  fi
}

copy_fixture() {
  local name="$1"
  local dest="$BASE_DIR/$name"
  cp -R "$SRC_DIR" "$dest"
  printf '%s\n' "$dest"
}

workspace_totals() {
  local project_root="$1"
  local total_files=0
  local total_bytes=0
  while IFS= read -r project_dir; do
    [[ -n "$project_dir" ]] || continue
    if [[ -d "$project_dir/node_modules" ]]; then
      local files bytes
      files="$(find "$project_dir/node_modules" -type f | wc -l | tr -d ' ')"
      bytes="$(find "$project_dir/node_modules" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
      total_files=$((total_files + ${files:-0}))
      total_bytes=$((total_bytes + ${bytes:-0}))
    fi
  done < <(find "$project_root/apps" "$project_root/packages" -mindepth 1 -maxdepth 3 -name package.json 2>/dev/null | xargs -I{} dirname "{}" | sort)

  echo "$total_files $total_bytes"
}

shared_cache_stats() {
  local files bytes
  files="$(find "$SHARED_CACHE_DIR" -type f | wc -l | tr -d ' ')"
  bytes="$(find "$SHARED_CACHE_DIR" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
  echo "${files:-0} ${bytes:-0}"
}

run_scenario() {
  local name="$1"
  local project_root="$2"
  local output_file="$BASE_DIR/${name}.log"
  local status real user sys root_lock workspace_files workspace_bytes cache_files cache_bytes
  if (
    cd "$project_root"
    export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
    /usr/bin/time -p sh -c '
      if [ -x "'"$ROOT"'/target/debug/mg" ]; then
        exec "'"$ROOT"'/target/debug/mg" install
      fi
      exec cargo run --manifest-path "'"$ROOT"'/cli/Cargo.toml" --bin mg --no-default-features --features web -- install
    '
  ) >"$output_file" 2>&1; then
    status=0
  else
    status=$?
  fi

  real="$(grep -E '^real ' "$output_file" | tail -n 1 | awk '{print $2}')"
  user="$(grep -E '^user ' "$output_file" | tail -n 1 | awk '{print $2}')"
  sys="$(grep -E '^sys ' "$output_file" | tail -n 1 | awk '{print $2}')"
  if [[ -f "$project_root/mg.lock" ]]; then
    root_lock="yes"
  else
    root_lock="no"
  fi
  read -r workspace_files workspace_bytes < <(workspace_totals "$project_root")
  read -r cache_files cache_bytes < <(shared_cache_stats)

  {
    echo "### ${name}"
    echo
    echo "- project: $project_root"
    echo "- exit-status: $status"
    echo "- real(s): ${real:-n/a}"
    echo "- user(s): ${user:-n/a}"
    echo "- sys(s): ${sys:-n/a}"
    echo "- root mg.lock: $root_lock"
    echo "- workspace node_modules files: ${workspace_files:-0}"
    echo "- workspace node_modules bytes: ${workspace_bytes:-0}"
    echo "- shared cache files: ${cache_files:-0}"
    echo "- shared cache bytes: ${cache_bytes:-0}"
    echo
    echo '```text'
    sed -n '1,160p' "$output_file"
    echo '```'
    echo
  } >> "$REPORT"
}

PROJECT_COLD="$(copy_fixture project-cold)"
run_scenario "cold-project-empty-shared-cache" "$PROJECT_COLD"
run_scenario "warm-reinstall-same-project" "$PROJECT_COLD"

PROJECT_FRESH_WARM="$(copy_fixture project-fresh-warm-shared)"
run_scenario "fresh-project-warm-shared-cache" "$PROJECT_FRESH_WARM"

echo "workspace benchmark report: $REPORT"
