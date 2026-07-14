#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"
REPORT="$LAB_DIR/benchmarks/framework-workflows-$STAMP.md"
BASE_DIR="$(mktemp -d "/private/tmp/core-web-frameworks.${STAMP}.XXXXXX")"
SHARED_CACHE_DIR="$BASE_DIR/shared-cache"
mkdir -p "$SHARED_CACHE_DIR"

FRAMEWORKS=(
  react-vite
  nextjs
  vue-vite
  nuxt
  sveltekit
  remix
  astro
  angular
  solidjs
  vanilla
  qwik
)

ADD_PACKAGE="tiny-invariant"

if [[ -x "$ROOT/target/debug/mg" ]]; then
  MG_BIN="$ROOT/target/debug/mg"
else
  echo "missing mg binary at $ROOT/target/debug/mg; build it first" >&2
  exit 1
fi

{
  echo "# Framework Workflow Lane"
  echo
  echo "- timestamp: $STAMP"
  echo "- mg-binary: $MG_BIN"
  echo "- shared-cache-dir: $SHARED_CACHE_DIR"
  echo "- frameworks: ${FRAMEWORKS[*]}"
  echo "- workflow: create -> install -> add -> install -> remove -> install -> dev"
  echo
} > "$REPORT"

shared_cache_stats() {
  local files bytes
  files="$(find "$SHARED_CACHE_DIR" -type f | wc -l | tr -d ' ')"
  bytes="$(find "$SHARED_CACHE_DIR" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
  echo "${files:-0} ${bytes:-0}"
}

node_modules_stats() {
  local root="$1"
  local files bytes
  if [[ ! -d "$root/node_modules" ]]; then
    echo "0 0"
    return
  fi
  files="$(find "$root/node_modules" -type f | wc -l | tr -d ' ')"
  bytes="$(find "$root/node_modules" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"
  echo "${files:-0} ${bytes:-0}"
}

run_timed() {
  local logfile="$1"
  shift
  if /usr/bin/time -p "$@" >"$logfile" 2>&1; then
    return 0
  fi
  return $?
}

extract_time() {
  local logfile="$1"
  grep -E '^real ' "$logfile" | tail -n 1 | awk '{print $2}'
}

lock_status() {
  local root="$1"
  local status="missing"
  if [[ -f "$root/mg.lock" ]]; then
    status="present"
  fi
  if [[ -f "$root/mg.lock" && -f "$root/mg.lock.sha256" ]]; then
    local actual expected
    actual="$(shasum -a 256 "$root/mg.lock" | awk '{print $1}')"
    expected="$(tr -d '[:space:]' < "$root/mg.lock.sha256")"
    if [[ "$actual" == "$expected" ]]; then
      status="present+sha256-ok"
    else
      status="present+sha256-mismatch"
    fi
  fi
  printf '%s\n' "$status"
}

dev_probe() {
  local project_dir="$1"
  local port="$2"
  local logfile="$3"
  (
    cd "$project_dir"
    export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
    "$MG_BIN" dev --host 127.0.0.1 --port "$port"
  ) >"$logfile" 2>&1 &
  local pid=$!
  local status="fail"
  local code="000"
  local total="0"

  for _ in $(seq 1 90); do
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
    sleep 1
    local probe
    probe="$(curl -s -o /dev/null -w '%{http_code} %{time_total}' "http://127.0.0.1:$port/" || true)"
    code="${probe%% *}"
    total="${probe##* }"
    if [[ -n "$code" && "$code" != "000" ]]; then
      status="ok"
      break
    fi
  done

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  printf '%s %s %s\n' "$status" "$code" "$total"
}

for index in "${!FRAMEWORKS[@]}"; do
  framework="${FRAMEWORKS[$index]}"
  project_dir="$BASE_DIR/${framework}-app"
  port=$((4315 + index))

  read -r cache_files_before cache_bytes_before < <(shared_cache_stats)

  create_log="$BASE_DIR/${framework}-create.log"
  install_log="$BASE_DIR/${framework}-install.log"
  add_log="$BASE_DIR/${framework}-add.log"
  add_install_log="$BASE_DIR/${framework}-add-install.log"
  remove_log="$BASE_DIR/${framework}-remove.log"
  remove_install_log="$BASE_DIR/${framework}-remove-install.log"
  frozen_log="$BASE_DIR/${framework}-frozen.log"
  audit_log="$BASE_DIR/${framework}-audit.log"
  dev_log="$BASE_DIR/${framework}-dev.log"

  create_status=0
  install_status=0
  add_status=0
  add_install_status=0
  remove_status=0
  remove_install_status=0
  frozen_status=0
  audit_status=0

  (
    cd "$BASE_DIR"
    export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
    run_timed "$create_log" "$MG_BIN" create "$framework" "$(basename "$project_dir")" --ts
  ) || create_status=$?

  if [[ $create_status -eq 0 ]]; then
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$install_log" "$MG_BIN" install
    ) || install_status=$?
  else
    install_status=999
  fi

  if [[ $install_status -eq 0 ]]; then
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$add_log" "$MG_BIN" add "$ADD_PACKAGE"
    ) || add_status=$?
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$add_install_log" "$MG_BIN" install
    ) || add_install_status=$?
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$remove_log" "$MG_BIN" remove "$ADD_PACKAGE"
    ) || remove_status=$?
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$remove_install_log" "$MG_BIN" install
    ) || remove_install_status=$?
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$frozen_log" "$MG_BIN" install --frozen
    ) || frozen_status=$?
    (
      cd "$project_dir"
      export MEGAGATE_SHARED_CACHE_DIR="$SHARED_CACHE_DIR"
      run_timed "$audit_log" "$MG_BIN" audit
    ) || audit_status=$?
  else
    add_status=999
    add_install_status=999
    remove_status=999
    remove_install_status=999
    frozen_status=999
    audit_status=999
  fi

  dev_status="skipped"
  dev_code="000"
  dev_time="0"
  if [[ $remove_install_status -eq 0 ]]; then
    read -r dev_status dev_code dev_time < <(dev_probe "$project_dir" "$port" "$dev_log")
  fi

  read -r nm_files nm_bytes < <(node_modules_stats "$project_dir")
  read -r cache_files_after cache_bytes_after < <(shared_cache_stats)
  lock_integrity="$(lock_status "$project_dir")"

  {
    echo "## $framework"
    echo
    echo "- project-dir: $project_dir"
    echo "- create-status: $create_status"
    echo "- create-real(s): $(extract_time "$create_log" 2>/dev/null || true)"
    echo "- install-status: $install_status"
    echo "- install-real(s): $(extract_time "$install_log" 2>/dev/null || true)"
    echo "- add-status: $add_status"
    echo "- add-real(s): $(extract_time "$add_log" 2>/dev/null || true)"
    echo "- add-install-status: $add_install_status"
    echo "- add-install-real(s): $(extract_time "$add_install_log" 2>/dev/null || true)"
    echo "- remove-status: $remove_status"
    echo "- remove-real(s): $(extract_time "$remove_log" 2>/dev/null || true)"
    echo "- remove-install-status: $remove_install_status"
    echo "- remove-install-real(s): $(extract_time "$remove_install_log" 2>/dev/null || true)"
    echo "- frozen-install-status: $frozen_status"
    echo "- frozen-install-real(s): $(extract_time "$frozen_log" 2>/dev/null || true)"
    echo "- audit-status: $audit_status"
    echo "- audit-real(s): $(extract_time "$audit_log" 2>/dev/null || true)"
    echo "- dev-status: $dev_status"
    echo "- dev-http-code: $dev_code"
    echo "- dev-time(s): $dev_time"
    echo "- dev-port: $port"
    echo "- mg.lock: $lock_integrity"
    echo "- node_modules files: $nm_files"
    echo "- node_modules bytes: $nm_bytes"
    echo "- shared-cache files before: $cache_files_before"
    echo "- shared-cache bytes before: $cache_bytes_before"
    echo "- shared-cache files after: $cache_files_after"
    echo "- shared-cache bytes after: $cache_bytes_after"
    echo "- shared-cache files delta: $((cache_files_after - cache_files_before))"
    echo "- shared-cache bytes delta: $((cache_bytes_after - cache_bytes_before))"
    echo
    echo "### Notes"
    echo
    if [[ "$dev_status" == "ok" ]]; then
      echo "- dev server responded on localhost"
    else
      echo "- dev server did not produce a valid localhost response"
    fi
    if [[ "$lock_integrity" == "present+sha256-ok" ]]; then
      echo "- lockfile sidecar hash verified"
    else
      echo "- lockfile sidecar hash missing or mismatched"
    fi
    echo
  } >> "$REPORT"
done

echo "framework workflow report: $REPORT"
