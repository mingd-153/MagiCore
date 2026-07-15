#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
FIXTURES_DIR="$LAB_DIR/fixtures"
STAMP="$(date '+%Y%m%d-%H%M%S')"

PM="${1:-}"
FIXTURE="${2:-react-vite-basic}"

if [[ -z "$PM" ]]; then
  echo "usage: $0 <mg|npm|pnpm|bun> [fixture]" >&2
  exit 1
fi

SRC_DIR="$FIXTURES_DIR/$FIXTURE"
if [[ ! -d "$SRC_DIR" ]]; then
  echo "unknown fixture: $FIXTURE" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "/private/tmp/core-web-lab-${PM}-${FIXTURE}.XXXXXX")"
DEST_DIR="$WORK_DIR/$FIXTURE"
REPORT="$LAB_DIR/benchmarks/${PM}-${FIXTURE}-${STAMP}.md"

cp -R "$SRC_DIR" "$DEST_DIR"

is_workspace_fixture() {
  [[ -f "$DEST_DIR/megagate.workspace.toml" ]] || grep -q '"workspaces"' "$DEST_DIR/package.json" 2>/dev/null
}

workspace_projects() {
  find "$DEST_DIR/apps" "$DEST_DIR/packages" -mindepth 1 -maxdepth 3 -name package.json 2>/dev/null \
    | xargs -I{} dirname "{}" \
    | sort
}

workspace_totals() {
  local total_files=0
  local total_bytes=0
  while IFS= read -r project_dir; do
    [[ -n "$project_dir" ]] || continue
    if [[ -d "$project_dir/node_modules" ]]; then
      files="$(find "$project_dir/node_modules" -type f | wc -l | tr -d ' ')"
      bytes="$(find "$project_dir/node_modules" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
      total_files=$((total_files + ${files:-0}))
      total_bytes=$((total_bytes + ${bytes:-0}))
    fi
  done < <(workspace_projects)

  echo "$total_files $total_bytes"
}

run_pm() {
  case "$PM" in
    mg)
      local mg_runner
      if [[ -x "$ROOT/target/debug/mg" ]]; then
        mg_runner="$ROOT/target/debug/mg"
      else
        mg_runner="cargo run --manifest-path \"$ROOT/cli/Cargo.toml\" --bin mg --no-default-features --features web --"
      fi
      (
        cd "$DEST_DIR"
        if [[ -x "$ROOT/target/debug/mg" ]]; then
          /usr/bin/time -p "$ROOT/target/debug/mg" install
        else
          /usr/bin/time -p cargo run --manifest-path "$ROOT/cli/Cargo.toml" --bin mg --no-default-features --features web -- install
        fi
      )
      ;;
    npm)
      (
        cd "$DEST_DIR"
        /usr/bin/time -p npm install
      )
      ;;
    pnpm)
      (
        cd "$DEST_DIR"
        /usr/bin/time -p pnpm install
      )
      ;;
    bun)
      (
        cd "$DEST_DIR"
        /usr/bin/time -p bun install
      )
      ;;
    *)
      echo "unsupported pm: $PM" >&2
      exit 1
      ;;
  esac
}

STATUS=0
{
  echo "# PM Smoke"
  echo
  echo "- timestamp: $STAMP"
  echo "- pm: $PM"
  echo "- fixture: $FIXTURE"
  echo "- workdir: $DEST_DIR"
  echo
  echo '```text'
  run_pm || STATUS=$?
  echo '```'
  echo
  echo "- exit-status: $STATUS"
  echo
  echo "## Output layout"
  echo
  if [[ -d "$DEST_DIR/node_modules" ]]; then
    echo "- node_modules: yes"
    echo "- node_modules files: $(find "$DEST_DIR/node_modules" -type f | wc -l | tr -d ' ')"
    echo "- node_modules bytes: $(find "$DEST_DIR/node_modules" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
  else
    echo "- node_modules: no"
  fi
  if [[ -f "$DEST_DIR/mg.lock" ]]; then
    echo "- mg.lock: yes"
  else
    echo "- mg.lock: no"
  fi
  if [[ -f "$DEST_DIR/package-lock.json" ]]; then
    echo "- package-lock.json: yes"
  else
    echo "- package-lock.json: no"
  fi
  if [[ -f "$DEST_DIR/pnpm-lock.yaml" ]]; then
    echo "- pnpm-lock.yaml: yes"
  else
    echo "- pnpm-lock.yaml: no"
  fi
  if [[ -f "$DEST_DIR/bun.lock" || -f "$DEST_DIR/bun.lockb" ]]; then
    echo "- bun lock: yes"
  else
    echo "- bun lock: no"
  fi
  if is_workspace_fixture; then
    read -r workspace_total_files workspace_total_bytes < <(workspace_totals)
    echo "- workspace node_modules files total: ${workspace_total_files:-0}"
    echo "- workspace node_modules bytes total: ${workspace_total_bytes:-0}"
    echo
    echo
    echo "## Workspace layout"
    echo
    while IFS= read -r project_dir; do
      [[ -n "$project_dir" ]] || continue
      rel="${project_dir#$DEST_DIR/}"
      echo "- project: $rel"
      if [[ -d "$project_dir/node_modules" ]]; then
        echo "  - node_modules: yes"
        echo "  - node_modules files: $(find "$project_dir/node_modules" -type f | wc -l | tr -d ' ')"
        echo "  - node_modules bytes: $(find "$project_dir/node_modules" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
      else
        echo "  - node_modules: no"
      fi
      if [[ -f "$project_dir/mg.lock" ]]; then
        echo "  - mg.lock: yes"
      else
        echo "  - mg.lock: no"
      fi
    done < <(workspace_projects)
  fi
} > "$REPORT" 2>&1

echo "pm smoke report: $REPORT"
exit "$STATUS"
