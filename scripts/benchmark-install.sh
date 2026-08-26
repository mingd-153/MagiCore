#!/usr/bin/env bash
# benchmark-install.sh — Cold/warm install benchmark: mgc vs npm vs pnpm.
#
# Hai chế độ:
#   REGISTRY_MODE=real  (mặc định) — bắn thẳng registry.npmjs.org (nhiễu mạng thật)
#   REGISTRY_MODE=local            — mọi tool trỏ vào mgc-registry local (upstream npmjs,
#                                    cache tại store) ⇒ so TỐC ĐỘ TOOL THUẦN, không nhiễu mạng
#
# MULTI_PROJECTS=N (vd 5) — cài thêm N project giống nhau dùng chung cache/store:
#   đo thời gian từng project + tổng disk (node_modules ×N + store/cache) ⇒ thể hiện CAS dedup.
#
# Usage: REGISTRY_MODE=local MULTI_PROJECTS=5 scripts/benchmark-install.sh [runs_cold] [runs_warm]
# Output: benchmarks/install/results-<stamp>.md (+ stdout)
set -euo pipefail

RUNS_COLD="${1:-3}"
RUNS_WARM="${2:-3}"
REGISTRY_MODE="${REGISTRY_MODE:-real}"
MULTI="${MULTI_PROJECTS:-0}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MGC_BIN="$ROOT/target/release/mgc"
REG_BIN="$ROOT/target/release/mgc-registry"
STAMP="$(date +%Y%m%d-%H%M%S)"
WORK="$(mktemp -d /tmp/mgc-pm-bench.XXXXXX)"
FIXTURE="$WORK/fixture"
OUT_FILE="$ROOT/benchmarks/install/results-$STAMP.md"

command -v hyperfine >/dev/null || { echo "hyperfine required" >&2; exit 1; }
[[ -x "$MGC_BIN" ]] || { echo "build: cargo build --release -p mgc --bin mgc" >&2; exit 1; }
command -v pnpm >/dev/null || { echo "pnpm required" >&2; exit 1; }
[[ "$REGISTRY_MODE" == "local" && ! -x "$REG_BIN" ]] && \
  { echo "build: cargo build --release -p mgc-registry-server --bin mgc-registry" >&2; exit 1; }

mkdir -p "$FIXTURE" "$ROOT/benchmarks/install"

cat > "$FIXTURE/package.json" <<'EOF'
{
  "name": "pm-bench-fixture",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "react": "18.3.1",
    "react-dom": "18.3.1",
    "express": "4.19.2",
    "lodash": "4.17.21",
    "axios": "1.7.7",
    "typescript": "5.5.4",
    "zod": "3.23.8",
    "date-fns": "3.6.0",
    "chalk": "5.3.0",
    "commander": "12.1.0"
  }
}
EOF

# ---------------------------------------------------------------- registry local
REG_URL=""
REG_PID=""
REG_PORT=""
start_local_registry() {
  local port store
  store="/tmp/mgc-bench-reg-store"  # persist — tránh bị npmjs throttle mỗi lần chạy
  port=$(python3 - <<'PY'
import socket
s = socket.socket(); s.bind(("127.0.0.1", 0))
print(s.getsockname()[1]); s.close()
PY
)
  python3 - "$REG_BIN" "$port" "$store" "$WORK/reg.log" <<'PY'
import subprocess, sys
bin_, port, store, log = sys.argv[1:5]
subprocess.Popen(
    [bin_, "--port", port, "--store-dir", store,
     "--admin-token", "e2e-admin-token",
     "--upstream", "https://registry.npmjs.org"],
    stdout=open(log, "ab"), stderr=subprocess.STDOUT,
    start_new_session=True,   # tách process-group — sống sót qua kill-group của hyperfine
)
PY
  REG_PID=$(pgrep -f "mgc-registry --port $port" | head -1)
  trap '[[ -n "$REG_PID" ]] && kill "$REG_PID" 2>/dev/null' EXIT
  for _ in $(seq 1 50); do
    if ! python3 -c "import socket;s=socket.socket();s.bind(('127.0.0.1',$port));s.close()" 2>/dev/null; then
      REG_URL="http://127.0.0.1:$port"; break
    fi
    sleep 0.2
  done
  [[ -n "$REG_URL" ]] || { echo "registry failed to start (see $WORK/reg.log)" >&2; exit 1; }
  REG_PORT="$port"
  echo "[bench] registry ready: $REG_URL"

  # Auth cho npm/pnpm qua .npmrc trong project (env var không chứa được '/' trong tên)
  local port="${REG_URL##*:}"
  cat > "$FIXTURE/.npmrc" <<NPMRC
registry=$REG_URL
//127.0.0.1:$port/:_authToken=e2e-admin-token
always-auth=true
NPMRC

  # Prime: 1 lần cài ngâm để registry kéo sẵn cây từ upstream — sau đó timed runs thuần local
  mkdir -p "$WORK/prime-home"
  env HOME="$WORK/prime-home" MAGICORE_WEB_REGISTRY_URL="$REG_URL" \
      MAGICORE_WEB_REGISTRY_TOKEN=e2e-admin-token MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST=1 \
      bash -c "cd '$FIXTURE' && $MGC_BIN --core web install" >/dev/null 2>&1 || true
}

[[ "$REGISTRY_MODE" == "local" ]] && start_local_registry

# ---------------------------------------------------------------- per-tool env + cmd
tool_env() {
  # $1=tool $2=home  → in chuỗi "K=V K=V" (luôn exit 0 — set -e giết script nếu return 1)
  local tool="$1" home="$2"
  case "$tool" in
    mgc)
      printf 'HOME=%s' "$home"
      if [[ -n "$REG_URL" ]]; then
        printf ' MAGICORE_WEB_REGISTRY_URL=%s MAGICORE_WEB_REGISTRY_TOKEN=e2e-admin-token MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST=1' "$REG_URL"
      fi
      ;;
    npm|pnpm)
      printf 'npm_config_cache=%s/cache XDG_CACHE_HOME=%s/xdg-cache XDG_DATA_HOME=%s/xdg-data' "$home" "$home" "$home"
      if [[ -n "$REG_URL" ]]; then
        printf ' npm_config_registry=%s' "$REG_URL"
      fi
      if [[ "$tool" == "pnpm" ]]; then
        printf ' npm_config_store_dir=%s/store' "$home"
      fi
      ;;
  esac
  return 0
}

install_cmd() {
  case "$1" in
    mgc)  echo "$MGC_BIN --core web install" ;;
    npm)  echo "npm install --no-audit --no-fund --loglevel=error" ;;
    pnpm) echo "pnpm install --silent" ;;
  esac
}

ensure_registry() {
  # Registry từng chết ngầm giữa chừng (không panic trong log) — tự hồi sinh cùng port/store
  if ! python3 -c "import socket;s=socket.socket();s.connect(('127.0.0.1',$REG_PORT));s.close()" 2>/dev/null; then
    echo "[bench] !! registry died — respawning on :$REG_PORT"
    RUST_BACKTRACE=1 "$REG_BIN" --port "$REG_PORT" --store-dir "/tmp/mgc-bench-reg-store" \
      --admin-token e2e-admin-token --upstream https://registry.npmjs.org \
      >>"$WORK/reg.log" 2>&1 &
    REG_PID=$!
    sleep 1
  fi
}

run_tool() {
  local tool="$1" mode="$2" runs="$3" proj="$4" tag="${5:-s1}"
  ensure_registry
  local home="$WORK/$tool-$mode-home$proj"
  mkdir -p "$home"
  local env_str cache_dir
  env_str="$(tool_env "$tool" "$home")"
  case "$tool" in
    mgc)  cache_dir="$home/.magicore" ;;
    npm)  cache_dir="$home/cache" ;;
    pnpm) cache_dir="$home/cache" ;;
  esac

  local cmd="env $env_str bash -c 'cd \"$proj\" && $(install_cmd "$tool")'"

  if [[ "$mode" == "cold" ]]; then
    if ! hyperfine --runs "$runs" --warmup 0 \
      --prepare "rm -rf '$proj/node_modules' '$proj/.magicore' '$cache_dir' '$home/xdg-cache' '$home/xdg-data'" \
      --export-json "$WORK/$tool-$mode-$tag.json" \
      "$cmd" > "$WORK/$tool-$mode-$tag.out" 2>&1; then
      echo "[bench] !! $tool $mode FAILED — output:"
      cat "$WORK/$tool-$mode-$tag.out"
      exit 1
    fi
  else
    env $env_str bash -c "cd '$proj' && $(install_cmd "$tool")" >/dev/null 2>&1 || true
    if ! hyperfine --runs "$runs" --warmup 0 \
      --prepare "rm -rf '$proj/node_modules'" \
      --export-json "$WORK/$tool-$mode-$tag.json" \
      "$cmd" > "$WORK/$tool-$mode-$tag.out" 2>&1; then
      echo "[bench] !! $tool $mode FAILED — output:"
      cat "$WORK/$tool-$mode-$tag.out"
      exit 1
    fi
  fi
}

disk_val() {
  case "$1" in
    mgc)  echo "$DISK_MGC" ;;
    npm)  echo "$DISK_NPM" ;;
    pnpm) echo "$DISK_PNPM" ;;
  esac
}

json_mean()   { python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(f\"{d['results'][0]['mean']:.2f}\")" "$1"; }
json_stddev() { python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(f\"{d['results'][0]['stddev']:.2f}\")" "$1"; }
disk_node_modules() { [[ -d "$1/node_modules" ]] && du -sk "$1/node_modules" | cut -f1 || echo 0; }

# ---------------------------------------------------------------- S1: timing single project
echo "[bench] mode=$REGISTRY_MODE · cold×$RUNS_COLD warm×$RUNS_WARM"
for t in mgc npm pnpm; do
  echo "[bench]   → $t cold"; run_tool "$t" cold "$RUNS_COLD" "$FIXTURE"
done
for t in mgc npm pnpm; do
  echo "[bench]   → $t warm"; run_tool "$t" warm "$RUNS_WARM" "$FIXTURE"
done

# ---------------------------------------------------------------- S2: multi-project CAS
declare_disk() { :; }  # placeholder giữ bash3.2 vui lòng — dùng biến thường
if [[ "$MULTI" -gt 0 ]]; then
  echo "[bench] multi-project ×$MULTI (shared cache/store)..."
  : > "$WORK/multi-times.txt"
  for i in $(seq 1 "$MULTI"); do
    for t in mgc npm pnpm; do
      local_home="$WORK/$t-multi-home"
      tproj="$WORK/proj-$t/p$i"
      mkdir -p "$local_home" "$tproj"
      cp "$FIXTURE/package.json" "$tproj/package.json"
      ensure_registry
      env_str="$(tool_env "$t" "$local_home")"
      start=$(python3 -c 'import time; print(time.time())')
      env $env_str bash -c "cd '$tproj' && $(install_cmd "$t")" >/dev/null 2>&1 || true
      end=$(python3 -c 'import time; print(time.time())')
      echo "$t $i $(python3 -c "print(f'{$end-$start:.2f}')")" >> "$WORK/multi-times.txt"
    done
  done
fi

# ---------------------------------------------------------------- disk totals
total_disk_kb() {
  # tổng node_modules của mọi project + toàn bộ home (cache/store) của tool
  local tool="$1" total=0 i
  local dirs=()
  if [[ "$MULTI" -gt 0 ]]; then
    for i in $(seq 1 "$MULTI"); do dirs+=("$WORK/proj-$tool/p$i"); done
  else
    dirs=("$FIXTURE")
  fi
  for d in "${dirs[@]}"; do
    [[ -d "$d/node_modules" ]] && total=$((total + $(du -sk "$d/node_modules" | cut -f1)))
  done
  [[ -d "$WORK/$tool-multi-home" ]] && total=$((total + $(du -sk "$WORK/$tool-multi-home" | cut -f1)))
  echo "$total"
}
# Disk VẬT LÝ thật = df-delta quanh phase multi của từng tool
# (du mù với APFS clone/hardlink — đếm sai cả mgc reflink lẫn pnpm hardlink)
avail_kb() { df -k "$WORK" | awk 'NR==2{print $4}'; }
DISK_MGC=0; DISK_NPM=0; DISK_PNPM=0
for t in mgc npm pnpm; do
  ensure_registry
  rm -rf "$WORK/proj-$t" "$WORK/$t-multi-home"
  local_before=$(avail_kb)
  for i in $(seq 1 "$MULTI"); do
    tproj="$WORK/proj-$t/p$i"
    mkdir -p "$tproj"
    cp "$FIXTURE/package.json" "$tproj/package.json"
    env_str="$(tool_env "$t" "$local_home")"
    local_home="$WORK/$t-multi-home"
    env $env_str bash -c "cd '$tproj' && $(install_cmd "$t")" >/dev/null 2>&1 || true
  done
  d=$(($(avail_kb) - local_before))
  case "$t" in
    mgc)  DISK_MGC=$((d < 0 ? -d : d));;
    npm)  DISK_NPM=$((d < 0 ? -d : d));;
    pnpm) DISK_PNPM=$((d < 0 ? -d : d));;
  esac
done

# ---------------------------------------------------------------- report
machine_line="$(sw_vers -productVersion 2>/dev/null) · $(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m) · $(sysctl -n hw.ncpu) cores"
{
  echo "# Install Benchmark — $STAMP"
  echo
  echo "- Mode: **$REGISTRY_MODE**$( [[ -n "$REG_URL" ]] && echo " (registry: $REG_URL, upstream npmjs, pre-warmed)" )"
  echo "- Machine: macOS $machine_line"
  echo "- Fixture: 10 direct deps pinned · Runs cold×$RUNS_COLD warm×$RUNS_WARM"
  [[ "$MULTI" -gt 0 ]] && echo "- Multi-project: ×$MULTI shared cache/store"
  echo "- mgc @ $(git -C "$ROOT" rev-parse --short HEAD) (release build)"
  echo
  echo "| Tool | Cold mean (s) | ± | Warm mean (s) | ± | Physical disk ×5 projects (KB) |"
  echo "|------|---------------|---|---------------|---|-----------------|"
  for t in mgc npm pnpm; do
    d="$(disk_val "$t")"
    printf '| %-4s | %s | %s | %s | %s | %s |\n' "$t" \
      "$(json_mean "$WORK/$t-cold-s1.json")" "$(json_stddev "$WORK/$t-cold-s1.json")" \
      "$(json_mean "$WORK/$t-warm-s1.json")" "$(json_stddev "$WORK/$t-warm-s1.json")" "$d"
  done

  if [[ "$MULTI" -gt 0 ]]; then
    echo
    echo "### Multi-project install time (s) — project thứ i"
    echo
    echo "| Project | mgc | npm | pnpm |"
    echo "|---------|-----|-----|------|"
    for i in $(seq 1 "$MULTI"); do
      m=$(awk '$2=='"$i"'&&$1=="mgc"{print $3}' "$WORK/multi-times.txt")
      n=$(awk '$2=='"$i"'&&$1=="npm"{print $3}' "$WORK/multi-times.txt")
      pp=$(awk '$2=='"$i"'&&$1=="pnpm"{print $3}' "$WORK/multi-times.txt")
      printf '| %d | %s | %s | %s |\n' "$i" "${m:-n/a}" "${n:-n/a}" "${pp:-n/a}"
    done
  fi
} | tee "$OUT_FILE"

[[ -n "$REG_PID" ]] && kill "$REG_PID" 2>/dev/null
echo
echo "[bench] saved → $OUT_FILE"
echo "[bench] workdir: $WORK"
