#!/usr/bin/env bash
# benchmark-install.sh — Cold/warm install benchmark: mgc vs npm vs pnpm.
# So sánh thời gian install trên cùng fixture, cache hoàn toàn cô lập qua HOME/npm_config_cache.
#
# Usage: scripts/benchmark-install.sh [runs_cold] [runs_warm]
# Output: markdown table vào benchmarks/install/results-<date>.md (+ stdout)
#
# Yêu cầu: target/release/mgc đã build; node/npm/pnpm/hyperfine cài sẵn.
set -euo pipefail

RUNS_COLD="${1:-2}"
RUNS_WARM="${2:-3}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MGC_BIN="$ROOT/target/release/mgc"
STAMP="$(date +%Y%m%d-%H%M%S)"
WORK="$(mktemp -d /tmp/mgc-pm-bench.XXXXXX)"
FIXTURE="$WORK/fixture"
OUT_FILE="$ROOT/benchmarks/install/results-$STAMP.md"

command -v hyperfine >/dev/null || { echo "hyperfine required (brew install hyperfine)" >&2; exit 1; }
[[ -x "$MGC_BIN" ]] || { echo "build release first: cargo build --release -p mgc --bin mgc" >&2; exit 1; }
command -v pnpm >/dev/null || { echo "pnpm required" >&2; exit 1; }

mkdir -p "$FIXTURE" "$ROOT/benchmarks/install"

# Fixture web-app đại diện — version PIN CỨNG để mọi tool giải cùng một cây.
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

run_tool() {
  local tool="$1" mode="$2" runs="$3"
  local home="$WORK/$tool-$mode-home"
  mkdir -p "$home"

  # Đường cache/store của từng tool (nằm trong $home → cô lập tuyệt đối)
  local cache_dir="" store_dir=""
  case "$tool" in
    mgc)  cache_dir="$home/.magicore" ;;
    npm)  cache_dir="$home/cache" ;;
    pnpm) cache_dir="$home/cache"; store_dir="$home/store" ;;
  esac

  local -a extra_env=()
  case "$tool" in
    mgc)  extra_env=(HOME="$home") ;;
    npm)  extra_env=(npm_config_cache="$cache_dir") ;;
    pnpm) extra_env=(npm_config_cache="$cache_dir" npm_config_store_dir="$store_dir") ;;
  esac

  local cmd="env ${extra_env[*]} bash -c 'cd \"$FIXTURE\" && $(install_cmd "$tool")'"

  if [[ "$mode" == "cold" ]]; then
    # COLD: trước MỖI lần đo → xoá cả node_modules lẫn cache/store (khôi phục lại rỗng)
    echo "[bench]   → $tool cold ×$runs"
    hyperfine --runs "$runs" --warmup 0 --show-output \
      --prepare "rm -rf '$FIXTURE/node_modules' '$FIXTURE/.magicore' '$cache_dir' '$home/xdg-cache' '$home/xdg-data'" \
      --export-json "$WORK/$tool-$mode.json" \
      "$cmd" | tail -5
  else
    # WARM: 1 lần ngầm nạp cache/store, sau đó mỗi lần đo chỉ xoá node_modules
    env "${extra_env[@]}" bash -c "cd '$FIXTURE' && $(install_cmd "$tool")" >/dev/null 2>&1 || true
    hyperfine --style basic --runs "$runs" --warmup 0 \
      --prepare "rm -rf '$FIXTURE/node_modules'" \
      --export-json "$WORK/$tool-$mode.json" \
      "$cmd" > /dev/null
  fi
}

install_cmd() {
  case "$1" in
    mgc)  echo "$MGC_BIN --core web install" ;;
    npm)  echo "npm install --no-audit --no-fund --loglevel=error" ;;
    pnpm) echo "pnpm install --silent" ;;
  esac
}

json_mean() { python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(f\"{d['results'][0]['mean']:.2f}\")" "$1"; }
json_stddev() { python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(f\"{d['results'][0]['stddev']:.2f}\")" "$1"; }

disk_val() {
  case "$1" in
    mgc)  echo "$DISK_MGC" ;;
    npm)  echo "$DISK_NPM" ;;
    pnpm) echo "$DISK_PNPM" ;;
  esac
}

disk_node_modules() {
  [[ -d "$FIXTURE/node_modules" ]] && du -sk "$FIXTURE/node_modules" | cut -f1 || echo 0
}

echo "[bench] cold phase (cache/store wiped mỗi lần)..."
for t in mgc npm pnpm; do run_tool "$t" cold "$RUNS_COLD"; done

echo "[bench] warm phase (chạy nạp cache trước, chỉ xoá node_modules giữa các lần)..."
for t in mgc npm pnpm; do run_tool "$t" warm "$RUNS_WARM"; done

echo "[bench] disk usage..."
DISK_MGC=0; DISK_NPM=0; DISK_PNPM=0
for t in mgc npm pnpm; do
  local_home="$WORK/$t-warm-home"
  rm -rf "$FIXTURE/node_modules"
  case "$t" in
    mgc)  HOME="$local_home" bash -c "cd '$FIXTURE' && $(install_cmd mgc)" >/dev/null 2>&1;;
    npm)  npm_config_cache="$local_home/cache" bash -c "cd '$FIXTURE' && $(install_cmd npm)" >/dev/null 2>&1;;
    pnpm) npm_config_cache="$local_home/cache" npm_config_store_dir="$local_home/store" bash -c "cd '$FIXTURE' && $(install_cmd pnpm)" >/dev/null 2>&1;;
  esac
  case "$t" in
    mgc)  DISK_MGC="$(disk_node_modules)";;
    npm)  DISK_NPM="$(disk_node_modules)";;
    pnpm) DISK_PNPM="$(disk_node_modules)";;
  esac
done

{
  echo "# Install Benchmark — $STAMP"
  echo
  echo "- Machine: $(sw_vers -productVersion 2>/dev/null || uname -sr) · $(sysctl -n hw.model 2>/dev/null || uname -m) · $(sysctl -n hw.ncpu) cores"
  echo "- Fixture: 10 direct deps (react, express, typescript…) — version pinned"
  echo "- Runs: cold ×$RUNS_COLD · warm ×$RUNS_WARM · caches isolated via HOME / npm_config_cache"
  echo "- mgc binary: **release** build @ $(git -C "$ROOT" rev-parse --short HEAD)"
  echo
  echo "| Tool | Cold mean (s) | ± | Warm mean (s) | ± | node_modules disk (KB) |"
  echo "|------|---------------|---|---------------|---|------------------------|"
  for t in mgc npm pnpm; do
    printf '| %-4s | %s | %s | %s | %s | %s |\n' \
      "$t" \
      "$(json_mean "$WORK/$t-cold.json")" "$(json_stddev "$WORK/$t-cold.json")" \
      "$(json_mean "$WORK/$t-warm.json")" "$(json_stddev "$WORK/$t-warm.json")" \
      "$(disk_val "$t")"
  done
} | tee "$OUT_FILE"

echo
echo "[bench] saved → $OUT_FILE"
echo "[bench] workdir giữ lại để soi: $WORK (xoá tay khi xong)"
