#!/usr/bin/env bash
# MegaGate benchmark harness
# Strict, success-path-only benchmarking for core-web vs major package managers.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
MG_BIN="${MG_BIN:-$ROOT_DIR/target/release/mg}"
TMP_ROOT="${TMPDIR:-/tmp}/megagate-bench-$$"
RESULTS_MD="${ROOT_DIR}/benchmark_brutal_results_$(date +%Y%m%d_%H%M%S).md"
RESULTS_JSON="${RESULTS_MD%.md}.json"
STATUS_TSV="${RESULTS_MD%.md}.status.tsv"
BENCH_MODE="${BENCH_MODE:-full}"
BENCH_LANES="${BENCH_LANES:-}"
BENCH_PMS="${BENCH_PMS:-mg,bun,pnpm,npm,yarn}"
BENCH_RUNS="${BENCH_RUNS:-5}"
BENCH_WARMUP="${BENCH_WARMUP:-1}"
DEV_TIMEOUT_SECONDS="${DEV_TIMEOUT_SECONDS:-12}"
START_TIMEOUT_SECONDS="${START_TIMEOUT_SECONDS:-12}"
BACKEND_TIMEOUT_SECONDS="${BACKEND_TIMEOUT_SECONDS:-90}"
ENABLE_HEAVY_PROFILE="${ENABLE_HEAVY_PROFILE:-1}"
KEEP_TMP_ROOT="${KEEP_TMP_ROOT:-0}"
CONTINUE_ON_FAILURE="${CONTINUE_ON_FAILURE:-1}"
INTERRUPTED=0
FAILURES=0

ALL_PMS=(mg bun pnpm npm yarn)
ALL_LANES=(
  cold-install
  empty-cache-install
  warm-install
  add-single
  add-single-mutate-only
  add-multiple
  remove-single
  remove-single-mutate-only
  list
  why
  build
  dev-startup
  start-startup
  monorepo-install
  mg-create-web
  mg-create-web-rich
  heavy-cold-install
  heavy-empty-cache-install
  heavy-warm-install
  heavy-build
  heavy-dev-startup
  backend-go-echo
  native-go-echo-baseline
  backend-rust-axum
  native-rust-axum-baseline
  backend-python-fastapi
  native-python-fastapi-baseline
  backend-java-spring
  native-java-spring-baseline
)
SMOKE_LANES=(
  cold-install
  empty-cache-install
  add-single
  build
  dev-startup
  monorepo-install
  mg-create-web
  mg-create-web-rich
  heavy-cold-install
  backend-go-echo
)
MG_ONLY_LANES=(mg-create-web mg-create-web-rich backend-go-echo backend-rust-axum backend-python-fastapi backend-java-spring native-go-echo-baseline native-rust-axum-baseline native-python-fastapi-baseline native-java-spring-baseline)

cleanup() {
    if [[ "$KEEP_TMP_ROOT" == "1" || "$INTERRUPTED" == "1" ]]; then
        yellow "Keeping temp root: $TMP_ROOT"
        return
    fi
    rm -rf "$TMP_ROOT" 2>/dev/null || yellow "Could not fully remove temp root: $TMP_ROOT"
}
trap cleanup EXIT

handle_interrupt() {
    INTERRUPTED=1
    red "Benchmark interrupted. Partial report preserved."
}
trap handle_interrupt INT TERM

bold() { printf "\033[1m%s\033[0m\n" "$*"; }
green() { printf "\033[32m%s\033[0m\n" "$*"; }
yellow() { printf "\033[33m%s\033[0m\n" "$*"; }
red() { printf "\033[31m%s\033[0m\n" "$*"; }

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        red "Missing required command: $1"
        exit 1
    fi
}

contains_item() {
    local needle="$1"
    shift
    local item
    for item in "$@"; do
        [[ "$item" == "$needle" ]] && return 0
    done
    return 1
}

split_csv() {
    local raw="$1"
    SPLIT_CSV_RESULT=()
    IFS=',' read -r -a SPLIT_CSV_RESULT <<<"$raw"
}

resolve_selected_pms() {
    if [[ -z "$BENCH_PMS" ]]; then
        SELECTED_PMS=("${ALL_PMS[@]}")
        return
    fi
    split_csv "$BENCH_PMS"
    SELECTED_PMS=("${SPLIT_CSV_RESULT[@]}")
}

resolve_selected_lanes() {
    if [[ -n "$BENCH_LANES" ]]; then
        split_csv "$BENCH_LANES"
        SELECTED_LANES=("${SPLIT_CSV_RESULT[@]}")
        return
    fi

    case "$BENCH_MODE" in
        smoke)
            SELECTED_LANES=("${SMOKE_LANES[@]}")
            BENCH_RUNS=1
            BENCH_WARMUP=0
            ;;
        full)
            SELECTED_LANES=("${ALL_LANES[@]}")
            ;;
        *)
            red "Unknown BENCH_MODE: $BENCH_MODE"
            exit 1
            ;;
    esac
}

lane_title() {
    case "$1" in
        cold-install) echo "COLD INSTALL" ;;
        empty-cache-install) echo "EMPTY CACHE INSTALL" ;;
        warm-install) echo "WARM INSTALL" ;;
        add-single) echo "ADD SINGLE" ;;
        add-single-mutate-only) echo "ADD SINGLE MUTATE ONLY" ;;
        add-multiple) echo "ADD MULTIPLE" ;;
        remove-single) echo "REMOVE SINGLE" ;;
        remove-single-mutate-only) echo "REMOVE SINGLE MUTATE ONLY" ;;
        list) echo "LIST" ;;
        why) echo "WHY" ;;
        build) echo "BUILD" ;;
        dev-startup) echo "DEV STARTUP" ;;
        start-startup) echo "START STARTUP" ;;
        monorepo-install) echo "MONOREPO INSTALL" ;;
        mg-create-web) echo "MG CREATE WEB" ;;
        mg-create-web-rich) echo "MG CREATE WEB RICH" ;;
        heavy-cold-install) echo "HEAVY COLD INSTALL" ;;
        heavy-empty-cache-install) echo "HEAVY EMPTY CACHE INSTALL" ;;
        heavy-warm-install) echo "HEAVY WARM INSTALL" ;;
        heavy-build) echo "HEAVY BUILD" ;;
        heavy-dev-startup) echo "HEAVY DEV STARTUP" ;;
        backend-go-echo) echo "BACKEND GO ECHO" ;;
        native-go-echo-baseline) echo "NATIVE GO ECHO BASELINE" ;;
        backend-rust-axum) echo "BACKEND RUST AXUM" ;;
        native-rust-axum-baseline) echo "NATIVE RUST AXUM BASELINE" ;;
        backend-python-fastapi) echo "BACKEND PYTHON FASTAPI" ;;
        native-python-fastapi-baseline) echo "NATIVE PYTHON FASTAPI BASELINE" ;;
        backend-java-spring) echo "BACKEND JAVA SPRING" ;;
        native-java-spring-baseline) echo "NATIVE JAVA SPRING BASELINE" ;;
        *) echo "$1" ;;
    esac
}

is_mg_only_lane() {
    contains_item "$1" "${MG_ONLY_LANES[@]}"
}

preflight() {
    bold "=== PRE-FLIGHT ==="
    require_cmd cargo
    require_cmd hyperfine
    require_cmd python3
    require_cmd node
    local pm
    for pm in "${SELECTED_PMS[@]}"; do
        require_cmd "$pm"
    done
    cargo build --release -p mg
    if [[ ! -x "$MG_BIN" ]]; then
        red "MegaGate binary not found at $MG_BIN"
        exit 1
    fi
    mkdir -p "$TMP_ROOT"
    : >"$STATUS_TSV"
    green "MG binary: $MG_BIN"
    green "Results:   $RESULTS_MD"
    green "Mode:      $BENCH_MODE"
    green "PMs:       ${SELECTED_PMS[*]}"
    green "Lanes:     ${SELECTED_LANES[*]}"
}

assert_help_contains() {
    local command="$1"
    local needle="$2"
    local output
    output="$("$MG_BIN" "$command" --help 2>&1)"
    if [[ "$output" != *"$needle"* ]]; then
        red "CLI surface mismatch: 'mg $command --help' missing '$needle'"
        exit 2
    fi
}

verify_cli_surface() {
    bold "=== VERIFY CLI SURFACE ==="
    assert_help_contains "install" "--ignore-scripts"
    assert_help_contains "install" "--frozen"
    assert_help_contains "add" "--no-save"
    assert_help_contains "add" "--no-install"
    assert_help_contains "add" "--optional"
    assert_help_contains "add" "--peer"
    assert_help_contains "create-web" "--ts"
    assert_help_contains "create-web" "--tailwindcss"
    assert_help_contains "create-web" "--monorepo"
    assert_help_contains "create-web" "--express"
    assert_help_contains "create-web" "--fastify"
    assert_help_contains "install-web" "--ignore-scripts"
    assert_help_contains "add-web" "--no-save"
    assert_help_contains "add-web" "--no-install"
    assert_help_contains "add-web" "--peer"
    assert_help_contains "remove-web" "Remove web dependency"
    assert_help_contains "remove-web" "--no-install"
    assert_help_contains "update-web" "--install"
    assert_help_contains "dev" "--host"
    assert_help_contains "dev" "--port"
    green "CLI surface checks passed"
}

write_fixture_package_json() {
    local target="$1"
    cat >"$target" <<'EOF'
{
  "name": "bench-web",
  "version": "1.0.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite --host 127.0.0.1 --port 4315",
    "build": "vite build",
    "preview": "vite preview --host 127.0.0.1 --port 4415"
  },
  "dependencies": {
    "axios": "^1.9.0",
    "clsx": "^2.1.1",
    "date-fns": "^4.1.0",
    "express": "^4.21.2",
    "framer-motion": "^12.23.12",
    "lucide-react": "^0.539.0",
    "lodash": "^4.17.21",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-hook-form": "^7.62.0",
    "react-router-dom": "^7.8.0",
    "tailwind-merge": "^3.3.1",
    "uuid": "^11.1.0",
    "zod": "^4.0.17"
  },
  "devDependencies": {
    "@types/react": "^18.3.12",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.7.0",
    "typescript": "^5.8.3",
    "vite": "^7.0.6"
  }
}
EOF
}

create_base_fixture() {
    local dir="$TMP_ROOT/fixtures/base-web"
    mkdir -p "$dir/src"
    write_fixture_package_json "$dir/package.json"
    cat >"$dir/mg.toml" <<'EOF'
name = "bench-web"
version = "0.1.0"
ecosystem = "web"
EOF
    cat >"$dir/tsconfig.json" <<'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "Bundler",
    "allowImportingTsExtensions": false,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true
  },
  "include": ["src"]
}
EOF
    cat >"$dir/index.html" <<'EOF'
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>MegaGate Bench</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
EOF
    cat >"$dir/src/main.tsx" <<'EOF'
import React from "react";
import ReactDOM from "react-dom/client";

function App() {
  return (
    <main style={{ minHeight: "100vh", display: "grid", placeItems: "center", fontFamily: "Inter, sans-serif" }}>
      <div>
        <h1>MegaGate benchmark fixture</h1>
        <p>Success path only.</p>
      </div>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
EOF
    cat >"$dir/vite.config.ts" <<'EOF'
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()]
});
EOF
}

create_heavy_fixture() {
    local dir="$TMP_ROOT/fixtures/heavy-web"
    mkdir -p "$dir/src"
    cat >"$dir/package.json" <<'EOF'
{
  "name": "bench-heavy",
  "version": "1.0.0",
  "private": true,
  "type": "module",
  "main": "src/index.js",
  "scripts": {
    "dev": "vite --host 127.0.0.1 --port 4315",
    "build": "vite build",
    "preview": "vite preview --host 127.0.0.1 --port 4415"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "next": "^14.1.0",
    "vue": "^3.4.0",
    "vue-router": "^4.2.0",
    "express": "^4.18.2",
    "axios": "^1.6.0",
    "lodash": "^4.17.21",
    "chalk": "^5.3.0",
    "moment": "^2.29.4",
    "uuid": "^9.0.0",
    "tslib": "^2.6.2",
    "commander": "^11.1.0",
    "prettier": "^3.1.0",
    "eslint": "^8.56.0",
    "webpack": "^5.89.0",
    "babel-core": "^6.26.3",
    "jest": "^29.7.0",
    "sinon": "^17.0.1",
    "async": "^3.2.5",
    "body-parser": "^1.20.2",
    "cors": "^2.8.5",
    "debug": "^4.3.4",
    "dotenv": "^16.3.1",
    "glob": "^10.3.10",
    "http-errors": "^2.0.0",
    "iconv-lite": "^0.6.3",
    "isarray": "^2.0.5",
    "js-yaml": "^4.1.0",
    "json5": "^2.2.3",
    "jsonwebtoken": "^9.0.2",
    "ms": "^2.1.3",
    "node-fetch": "^3.3.2",
    "once": "^1.4.0",
    "path-to-regexp": "^6.2.1",
    "qs": "^6.11.2",
    "raw-body": "^2.5.2",
    "readable-stream": "^4.5.2",
    "safe-buffer": "^5.2.1",
    "semver": "^7.5.4",
    "send": "^0.18.0",
    "serve-static": "^1.15.0",
    "statuses": "^2.0.1",
    "supports-color": "^9.4.0",
    "through2": "^4.0.2",
    "yargs": "^17.7.2",
    "zod": "^3.22.4",
    "helmet": "^7.1.0",
    "compression": "^1.7.4",
    "morgan": "^1.10.0",
    "cookie-parser": "^1.4.6",
    "date-fns": "^4.1.0",
    "clsx": "^2.1.1",
    "tailwind-merge": "^3.3.1",
    "lucide-react": "^0.539.0",
    "react-hook-form": "^7.62.0",
    "react-router-dom": "^7.8.0",
    "nanoid": "^5.1.5",
    "zustand": "^5.0.7",
    "jotai": "^2.12.5",
    "valibot": "^1.1.0",
    "sonner": "^2.0.7",
    "framer-motion": "^12.23.12"
  },
  "devDependencies": {
    "typescript": "^5.3.3",
    "@types/react": "^18.3.12",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.7.0",
    "vite": "^7.0.6"
  }
}
EOF
    cat >"$dir/mg.toml" <<'EOF'
name = "bench-heavy"
version = "0.1.0"
ecosystem = "web"
EOF
    cat >"$dir/tsconfig.json" <<'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true
  },
  "include": ["src"]
}
EOF
    cat >"$dir/index.html" <<'EOF'
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>MegaGate Heavy Bench</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
EOF
    cat >"$dir/src/index.js" <<'EOF'
console.log("bench-heavy");
export default {};
EOF
    cat >"$dir/src/main.tsx" <<'EOF'
import React from "react";
import ReactDOM from "react-dom/client";
import { z } from "zod";
import { clsx } from "clsx";

const schema = z.object({ name: z.string(), mode: z.literal("heavy") });
const data = schema.parse({ name: "MegaGate", mode: "heavy" });

function App() {
  return (
    <main className={clsx("bench-heavy")} style={{ minHeight: "100vh", display: "grid", placeItems: "center", fontFamily: "Inter, sans-serif" }}>
      <section>
        <h1>{data.name} heavy benchmark</h1>
        <p>Resolver, cache, materialization, CLI surface.</p>
      </section>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
EOF
    cat >"$dir/vite.config.ts" <<'EOF'
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()]
});
EOF
}

create_monorepo_fixture() {
    local dir="$TMP_ROOT/fixtures/base-mono"
    mkdir -p "$dir/apps/web/src" "$dir/packages/ui/src" "$dir/packages/config"
    cat >"$dir/package.json" <<'EOF'
{
  "name": "bench-mono",
  "version": "1.0.0",
  "private": true,
  "workspaces": ["apps/*", "packages/*"]
}
EOF
    cat >"$dir/mg.toml" <<'EOF'
name = "bench-mono"
version = "0.1.0"
ecosystem = "web"
mode = "monorepo"
EOF
    cat >"$dir/apps/web/package.json" <<'EOF'
{
  "name": "@bench/web",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "@bench/ui": "*",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "zod": "^4.0.17"
  }
}
EOF
    cat >"$dir/packages/ui/package.json" <<'EOF'
{
  "name": "@bench/ui",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "clsx": "^2.1.1",
    "tailwind-merge": "^3.3.1"
  }
}
EOF
    cat >"$dir/packages/config/package.json" <<'EOF'
{
  "name": "@bench/config",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "typescript": "^5.8.3"
  }
}
EOF
}

create_fixtures() {
    bold "=== CREATE FIXTURES ==="
    mkdir -p "$TMP_ROOT/fixtures"
    create_base_fixture
    create_heavy_fixture
    create_monorepo_fixture
    local pm
    for pm in "${SELECTED_PMS[@]}"; do
        mkdir -p "$TMP_ROOT/work/$pm"
    done
    green "Fixtures created"
}

write_runner() {
    local runner="$TMP_ROOT/runner.sh"
    cat >"$runner" <<EOF
#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$ROOT_DIR"
MG_BIN="$MG_BIN"
TMP_ROOT="$TMP_ROOT"
DEV_TIMEOUT_SECONDS="$DEV_TIMEOUT_SECONDS"
START_TIMEOUT_SECONDS="$START_TIMEOUT_SECONDS"
BACKEND_TIMEOUT_SECONDS="$BACKEND_TIMEOUT_SECONDS"

copy_fixture() {
    local fixture="\$1"
    local dest="\$2"
    rm -rf "\$dest"
    mkdir -p "\$dest"
    cp -R "\$TMP_ROOT/fixtures/\$fixture/." "\$dest/"
}

assert_file() {
    local path="\$1"
    [[ -e "\$path" ]] || { echo "missing required file: \$path" >&2; exit 91; }
}

assert_dir() {
    local path="\$1"
    [[ -d "\$path" ]] || { echo "missing required dir: \$path" >&2; exit 92; }
}

assert_any_file() {
    local first="\$1"
    local second="\$2"
    if [[ -e "\$first" || -e "\$second" ]]; then
        return 0
    fi
    echo "missing required files: \$first or \$second" >&2
    exit 94
}

wait_for_http() {
    local url="\$1"
    local attempts="\${2:-60}"
    python3 - "\$url" "\$attempts" <<'PY'
import sys, time, urllib.request
url = sys.argv[1]
attempts = int(sys.argv[2])
for _ in range(attempts):
    try:
        with urllib.request.urlopen(url, timeout=0.5) as r:
            if 200 <= r.status < 500:
                length = r.headers.get("Content-Length")
                if length and int(length) <= 65536:
                    body = r.read(int(length)).decode("utf-8", errors="ignore")
                    if "MegaGate Native Dev Server is running" in body:
                        sys.exit(2)
                sys.exit(0)
    except Exception:
        time.sleep(0.2)
sys.exit(1)
PY
}

kill_process_tree() {
    local pid="\$1"
    local child
    for child in \$(pgrep -P "\$pid" 2>/dev/null || true); do
        kill_process_tree "\$child"
    done
    kill "\$pid" 2>/dev/null || true
}

run_bg_and_probe() {
    local cmd="\$1"
    local url="\$2"
    local timeout_secs="\$3"
    local logfile="\$4"
    bash -lc "\$cmd" >"\$logfile" 2>&1 &
    local pid=\$!
    local start_ts
    start_ts=\$(date +%s)
    until wait_for_http "\$url" 1; do
        if (( \$(date +%s) - start_ts >= timeout_secs )); then
            cat "\$logfile" >&2 || true
            kill_process_tree "\$pid"
            wait \$pid 2>/dev/null || true
            exit 93
        fi
        sleep 0.2
    done
    kill_process_tree "\$pid"
    wait \$pid 2>/dev/null || true
}

run_mg_native_backend() {
    local framework="\$1"
    local port="\$2"
    rm -rf "\$workdir"
    mkdir -p "\$workdir"
    cd "\$workdir"
    "\$MG_BIN" create-web "\$framework" app --yes
    cd app
    "\$MG_BIN" install-web
    run_bg_and_probe "\$MG_BIN dev --core web --host 127.0.0.1 --port \$port" "http://127.0.0.1:\$port/api/health" "\$BACKEND_TIMEOUT_SECONDS" "\$workdir/dev.log"
}

run_mg_with_empty_shared_cache() {
    local fixture="\$1"
    copy_fixture "\$fixture" "\$workdir"
    cd "\$workdir"
    local isolated_cache="\$workdir/.empty-shared-cache"
    rm -rf "\$isolated_cache"
    mkdir -p "\$isolated_cache"
    MEGAGATE_SHARED_CACHE_DIR="\$isolated_cache" "\$MG_BIN" install --core web --ignore-scripts
}

run_native_backend_baseline() {
    local framework="\$1"
    local port="\$2"
    rm -rf "\$workdir"
    mkdir -p "\$workdir"
    cd "\$workdir"
    "\$MG_BIN" create-web "\$framework" app --yes
    cd app
    case "\$framework" in
      echo)
        go mod tidy
        run_bg_and_probe "PORT=\$port HOST=127.0.0.1 go run ./cmd/server" "http://127.0.0.1:\$port/api/health" "\$BACKEND_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      axum)
        cargo fetch
        run_bg_and_probe "PORT=\$port HOST=127.0.0.1 cargo run" "http://127.0.0.1:\$port/api/health" "\$BACKEND_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      fastapi)
        python3 -m venv .venv
        .venv/bin/pip install -r requirements.txt
        run_bg_and_probe "PORT=\$port HOST=127.0.0.1 .venv/bin/python -m src.main" "http://127.0.0.1:\$port/api/health" "\$BACKEND_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      spring-boot)
        mvn -q -DskipTests dependency:go-offline
        run_bg_and_probe "mvn spring-boot:run -Dspring-boot.run.arguments='--server.port=\$port --server.address=127.0.0.1'" "http://127.0.0.1:\$port/api/health" "\$BACKEND_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      *)
        echo "unknown native baseline framework: \$framework" >&2
        exit 99
        ;;
    esac
}

pm="\$1"
lane="\$2"
workdir="\$TMP_ROOT/work/\$pm/\$lane"

case "\$lane" in
  cold-install)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    assert_dir "\$workdir/node_modules"
    case "\$pm" in
      mg) assert_file "\$workdir/mg.lock" ;;
      bun) assert_any_file "\$workdir/bun.lock" "\$workdir/bun.lockb" ;;
      pnpm) assert_file "\$workdir/pnpm-lock.yaml" ;;
      npm) assert_file "\$workdir/package-lock.json" ;;
      yarn) assert_file "\$workdir/yarn.lock" ;;
    esac
    ;;
  empty-cache-install)
    case "\$pm" in
      mg)
        run_mg_with_empty_shared_cache "base-web"
        ;;
      bun)
        copy_fixture "base-web" "\$workdir"
        cd "\$workdir"
        BUN_INSTALL_CACHE_DIR="\$workdir/.bun-cache" bun install
        ;;
      pnpm)
        copy_fixture "base-web" "\$workdir"
        cd "\$workdir"
        pnpm install --ignore-scripts --store-dir "\$workdir/.pnpm-store"
        ;;
      npm)
        copy_fixture "base-web" "\$workdir"
        cd "\$workdir"
        npm install --cache "\$workdir/.npm-cache"
        ;;
      yarn)
        copy_fixture "base-web" "\$workdir"
        cd "\$workdir"
        YARN_CACHE_FOLDER="\$workdir/.yarn-cache" yarn install
        ;;
    esac
    assert_dir "\$workdir/node_modules"
    ;;
  warm-install)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    rm -rf node_modules
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    assert_dir "\$workdir/node_modules"
    ;;
  add-single)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" add dayjs --core web ;;
      bun) bun install && bun add dayjs ;;
      pnpm) pnpm install --ignore-scripts && pnpm add dayjs --ignore-scripts ;;
      npm) npm install && npm install dayjs ;;
      yarn) yarn install && yarn add dayjs ;;
    esac
    grep -q '"dayjs"' package.json
    ;;
  add-single-mutate-only)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" add dayjs --core web --no-install ;;
      bun) bun add dayjs --dry-run ;;
      pnpm) pnpm add dayjs --lockfile-only --ignore-scripts ;;
      npm) npm install dayjs --package-lock-only --ignore-scripts ;;
      yarn) yarn add dayjs --mode=skip-builds ;;
    esac
    [[ "\$pm" != "mg" ]] || grep -q '"dayjs"' package.json
    ;;
  add-multiple)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" add zustand jotai valibot sonner --core web ;;
      bun) bun install && bun add zustand jotai valibot sonner ;;
      pnpm) pnpm install --ignore-scripts && pnpm add zustand jotai valibot sonner --ignore-scripts ;;
      npm) npm install && npm install zustand jotai valibot sonner ;;
      yarn) yarn install && yarn add zustand jotai valibot sonner ;;
    esac
    grep -q '"zustand"' package.json
    grep -q '"jotai"' package.json
    grep -q '"valibot"' package.json
    grep -q '"sonner"' package.json
    ;;
  remove-single)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" remove zod --core web ;;
      bun) bun install && bun remove zod ;;
      pnpm) pnpm install --ignore-scripts && pnpm remove zod ;;
      npm) npm install && npm uninstall zod ;;
      yarn) yarn install && yarn remove zod ;;
    esac
    ! grep -q '"zod"' package.json
    ;;
  remove-single-mutate-only)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" remove zod --core web --no-install ;;
      bun) bun remove zod --dry-run ;;
      pnpm) pnpm remove zod --lockfile-only ;;
      npm) npm uninstall zod --package-lock-only --ignore-scripts ;;
      yarn) yarn remove zod --mode=skip-builds ;;
    esac
    ! grep -q '"zod"' package.json || [[ "\$pm" != "mg" ]]
    ;;
  list)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" list --core web >/dev/null ;;
      bun) bun install && bun pm ls >/dev/null ;;
      pnpm) pnpm install --ignore-scripts && pnpm list >/dev/null ;;
      npm) npm install && npm list --depth=0 >/dev/null ;;
      yarn) yarn install && yarn list --depth=0 >/dev/null ;;
    esac
    ;;
  why)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" why react --core web >/dev/null ;;
      bun) bun install && bun why react >/dev/null ;;
      pnpm) pnpm install --ignore-scripts && pnpm why react >/dev/null ;;
      npm) npm install && npm why react >/dev/null ;;
      yarn) yarn install && yarn why react >/dev/null ;;
    esac
    ;;
  build)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" build --core web ;;
      bun) bun install && bunx vite build ;;
      pnpm) pnpm install --ignore-scripts && pnpm exec vite build ;;
      npm) npm install && npx vite build ;;
      yarn) yarn install && yarn vite build ;;
    esac
    assert_dir "\$workdir/dist"
    assert_any_file "\$workdir/dist/main.js" "\$workdir/dist/index.html"
    ;;
  dev-startup)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg)
        "\$MG_BIN" install --core web --ignore-scripts
        run_bg_and_probe "\$MG_BIN dev --core web --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      bun)
        bun install
        run_bg_and_probe "bunx vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      pnpm)
        pnpm install --ignore-scripts
        run_bg_and_probe "pnpm exec vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      npm)
        npm install
        run_bg_and_probe "npx vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      yarn)
        yarn install
        run_bg_and_probe "yarn vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
    esac
    ;;
  start-startup)
    copy_fixture "base-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg)
        "\$MG_BIN" install --core web --ignore-scripts
        "\$MG_BIN" build --core web
        run_bg_and_probe "\$MG_BIN start --core web" "http://127.0.0.1:4315/" "\$START_TIMEOUT_SECONDS" "\$workdir/start.log"
        ;;
      bun)
        bun install
        bunx vite build >/dev/null
        run_bg_and_probe "bunx vite preview --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$START_TIMEOUT_SECONDS" "\$workdir/start.log"
        ;;
      pnpm)
        pnpm install --ignore-scripts
        pnpm exec vite build >/dev/null
        run_bg_and_probe "pnpm exec vite preview --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$START_TIMEOUT_SECONDS" "\$workdir/start.log"
        ;;
      npm)
        npm install
        npx vite build >/dev/null
        run_bg_and_probe "npx vite preview --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$START_TIMEOUT_SECONDS" "\$workdir/start.log"
        ;;
      yarn)
        yarn install
        yarn vite build >/dev/null
        run_bg_and_probe "yarn vite preview --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$START_TIMEOUT_SECONDS" "\$workdir/start.log"
        ;;
    esac
    ;;
  monorepo-install)
    copy_fixture "base-mono" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    assert_dir "\$workdir/node_modules"
    ;;
  heavy-cold-install)
    copy_fixture "heavy-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    assert_dir "\$workdir/node_modules"
    ;;
  heavy-empty-cache-install)
    case "\$pm" in
      mg)
        run_mg_with_empty_shared_cache "heavy-web"
        ;;
      bun)
        copy_fixture "heavy-web" "\$workdir"
        cd "\$workdir"
        BUN_INSTALL_CACHE_DIR="\$workdir/.bun-cache" bun install
        ;;
      pnpm)
        copy_fixture "heavy-web" "\$workdir"
        cd "\$workdir"
        pnpm install --ignore-scripts --store-dir "\$workdir/.pnpm-store"
        ;;
      npm)
        copy_fixture "heavy-web" "\$workdir"
        cd "\$workdir"
        npm install --cache "\$workdir/.npm-cache"
        ;;
      yarn)
        copy_fixture "heavy-web" "\$workdir"
        cd "\$workdir"
        YARN_CACHE_FOLDER="\$workdir/.yarn-cache" yarn install
        ;;
    esac
    assert_dir "\$workdir/node_modules"
    ;;
  heavy-warm-install)
    copy_fixture "heavy-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    rm -rf node_modules
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts ;;
      bun) bun install ;;
      pnpm) pnpm install --ignore-scripts ;;
      npm) npm install ;;
      yarn) yarn install ;;
    esac
    assert_dir "\$workdir/node_modules"
    ;;
  heavy-build)
    copy_fixture "heavy-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg) "\$MG_BIN" install --core web --ignore-scripts && "\$MG_BIN" build --core web ;;
      bun) bun install && bunx vite build ;;
      pnpm) pnpm install --ignore-scripts && pnpm exec vite build ;;
      npm) npm install && npx vite build ;;
      yarn) yarn install && yarn vite build ;;
    esac
    assert_dir "\$workdir/dist"
    assert_any_file "\$workdir/dist/main.js" "\$workdir/dist/index.html"
    ;;
  heavy-dev-startup)
    copy_fixture "heavy-web" "\$workdir"
    cd "\$workdir"
    case "\$pm" in
      mg)
        "\$MG_BIN" install --core web --ignore-scripts
        run_bg_and_probe "\$MG_BIN dev --core web --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      bun)
        bun install
        run_bg_and_probe "bunx vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      pnpm)
        pnpm install --ignore-scripts
        run_bg_and_probe "pnpm exec vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      npm)
        npm install
        run_bg_and_probe "npx vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
      yarn)
        yarn install
        run_bg_and_probe "yarn vite --host 127.0.0.1 --port 4315" "http://127.0.0.1:4315/" "\$DEV_TIMEOUT_SECONDS" "\$workdir/dev.log"
        ;;
    esac
    ;;
  mg-create-web)
    rm -rf "\$workdir"
    mkdir -p "\$workdir"
    cd "\$workdir"
    "\$MG_BIN" create-web react-vite app --ts --yes
    assert_dir "\$workdir/app"
    assert_file "\$workdir/app/package.json"
    ;;
  mg-create-web-rich)
    rm -rf "\$workdir"
    mkdir -p "\$workdir"
    cd "\$workdir"
    "\$MG_BIN" create-web react-vite app --ts --tailwindcss --yes
    assert_dir "\$workdir/app"
    assert_file "\$workdir/app/package.json"
    assert_file "\$workdir/app/mg.toml"
    assert_dir "\$workdir/app/src"
    assert_dir "\$workdir/app/src/assets"
    assert_dir "\$workdir/app/src/bridges"
    assert_dir "\$workdir/app/src/components"
    assert_dir "\$workdir/app/src/config"
    assert_dir "\$workdir/app/src/content"
    assert_dir "\$workdir/app/src/hooks"
    assert_dir "\$workdir/app/src/pages"
    assert_dir "\$workdir/app/src/router"
    assert_dir "\$workdir/app/src/styles"
    assert_dir "\$workdir/app/crates"
    assert_dir "\$workdir/app/public"
    assert_file "\$workdir/app/src/main.tsx"
    ;;
  backend-go-echo)
    [[ "\$pm" == "mg" ]] || { echo "native backend lanes are MG-only" >&2; exit 90; }
    run_mg_native_backend "echo" "4321"
    assert_file "\$workdir/app/go.mod"
    assert_file "\$workdir/app/go.sum"
    ;;
  native-go-echo-baseline)
    [[ "\$pm" == "mg" ]] || { echo "native baseline lanes are MG-only wrappers" >&2; exit 90; }
    run_native_backend_baseline "echo" "4331"
    assert_file "\$workdir/app/go.mod"
    assert_file "\$workdir/app/go.sum"
    ;;
  backend-rust-axum)
    [[ "\$pm" == "mg" ]] || { echo "native backend lanes are MG-only" >&2; exit 90; }
    run_mg_native_backend "axum" "4322"
    assert_file "\$workdir/app/Cargo.toml"
    assert_file "\$workdir/app/Cargo.lock"
    ;;
  native-rust-axum-baseline)
    [[ "\$pm" == "mg" ]] || { echo "native baseline lanes are MG-only wrappers" >&2; exit 90; }
    run_native_backend_baseline "axum" "4332"
    assert_file "\$workdir/app/Cargo.toml"
    assert_file "\$workdir/app/Cargo.lock"
    ;;
  backend-python-fastapi)
    [[ "\$pm" == "mg" ]] || { echo "native backend lanes are MG-only" >&2; exit 90; }
    run_mg_native_backend "fastapi" "4323"
    assert_file "\$workdir/app/requirements.txt"
    assert_dir "\$workdir/app/.venv"
    ;;
  native-python-fastapi-baseline)
    [[ "\$pm" == "mg" ]] || { echo "native baseline lanes are MG-only wrappers" >&2; exit 90; }
    run_native_backend_baseline "fastapi" "4333"
    assert_file "\$workdir/app/requirements.txt"
    assert_dir "\$workdir/app/.venv"
    ;;
  backend-java-spring)
    [[ "\$pm" == "mg" ]] || { echo "native backend lanes are MG-only" >&2; exit 90; }
    run_mg_native_backend "spring-boot" "4324"
    assert_file "\$workdir/app/pom.xml"
    ;;
  native-java-spring-baseline)
    [[ "\$pm" == "mg" ]] || { echo "native baseline lanes are MG-only wrappers" >&2; exit 90; }
    run_native_backend_baseline "spring-boot" "4334"
    assert_file "\$workdir/app/pom.xml"
    ;;
  *)
    echo "unknown lane: \$lane" >&2
    exit 99
    ;;
esac
EOF
    chmod +x "$runner"
}

append_report_header() {
    cat >"$RESULTS_MD" <<EOF
# MegaGate Brutal Benchmark Report

Date: $(date)
MG: $("$MG_BIN" --version 2>/dev/null || echo "dev")
Bun: $(bun --version 2>/dev/null || echo "N/A")
pnpm: $(pnpm --version 2>/dev/null || echo "N/A")
npm: $(npm --version 2>/dev/null || echo "N/A")
Yarn: $(yarn --version 2>/dev/null || echo "N/A")
Node: $(node --version 2>/dev/null || echo "N/A")
Rust: $(rustc --version 2>/dev/null || echo "N/A")

## Rules

- Success path only: any non-zero exit fails the lane.
- Localhost only: all dev/start probes use \`127.0.0.1\`.
- No auto-install side effects inside the benchmark harness.
- Comparisons only cover commands with a meaningful counterpart.
- Benchmark mode: \`$BENCH_MODE\`
- Selected PMs: \`${SELECTED_PMS[*]}\`
- Selected lanes: \`${SELECTED_LANES[*]}\`
- Backend dev timeout: \`$BACKEND_TIMEOUT_SECONDS seconds\`
- Raw timing data is exported to JSON at \`$(basename "$RESULTS_JSON")\`.
- Lane status is exported to TSV at \`$(basename "$STATUS_TSV")\`.

## Lanes

$(for lane in "${SELECTED_LANES[@]}"; do printf -- "- %s\n" "$lane"; done)

---

EOF
}

run_lane() {
    local title="$1"
    local lane="$2"
    shift 2
    local commands=()
    local names=()
    while [[ $# -gt 0 ]]; do
        names+=("$1")
        commands+=("bash $TMP_ROOT/runner.sh $1 $lane")
        shift
    done

    {
        echo "## $title"
        echo
    } >>"$RESULTS_MD"

    local hf_args=(
        --warmup "$BENCH_WARMUP"
        --runs "$BENCH_RUNS"
        --show-output
        --export-json "$TMP_ROOT/${lane}.json"
    )
    local i
    for i in "${!names[@]}"; do
        hf_args+=(-n "${names[$i]}" "${commands[$i]}")
    done

    if [[ "$INTERRUPTED" == "1" ]]; then
        echo -e "$lane\tSKIPPED\tinterrupted-before-start" >>"$STATUS_TSV"
        {
            echo "_Skipped: benchmark was interrupted before this lane started._"
            echo
        } >>"$RESULTS_MD"
        return 0
    fi

    set +e
    hyperfine "${hf_args[@]}" 2>&1 | tee -a "$RESULTS_MD"
    local status=$?
    set -e

    if [[ $status -eq 0 ]]; then
        echo -e "$lane\tPASS\tok" >>"$STATUS_TSV"
    else
        echo -e "$lane\tFAIL\thyperfine-exit-$status" >>"$STATUS_TSV"
        FAILURES=$((FAILURES + 1))
        {
            echo
            echo "_Lane failed with exit code $status._"
            echo
        } >>"$RESULTS_MD"
        if [[ "$CONTINUE_ON_FAILURE" != "1" ]]; then
            return "$status"
        fi
    fi
    echo >>"$RESULTS_MD"
}

merge_json_results() {
    python3 - "$TMP_ROOT" "$RESULTS_JSON" <<'PY'
import json
import os
import sys
tmp_root, out_path = sys.argv[1:3]
payload = {}
for name in sorted(os.listdir(tmp_root)):
    if not name.endswith(".json"):
        continue
    path = os.path.join(tmp_root, name)
    try:
        with open(path, "r", encoding="utf-8") as fh:
            payload[name[:-5]] = json.load(fh)
    except Exception:
        payload[name[:-5]] = {"error": "invalid-or-incomplete-json"}
with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, indent=2)
PY
}

append_footer() {
    cat >>"$RESULTS_MD" <<EOF
## Lane Status

EOF
    if [[ -s "$STATUS_TSV" ]]; then
        while IFS=$'\t' read -r lane state detail; do
            printf -- "- \`%s\`: %s (%s)\n" "$lane" "$state" "$detail" >>"$RESULTS_MD"
        done <"$STATUS_TSV"
    else
        echo "- no lane status recorded" >>"$RESULTS_MD"
    fi

    local interrupted_label="no"
    [[ "$INTERRUPTED" == "1" ]] && interrupted_label="yes"

    cat >>"$RESULTS_MD" <<EOF

## Summary

- Interrupted: \`$interrupted_label\`
- Failures: \`$FAILURES\`

## Notes

- If a lane is missing timing data but appears in the lane-status section, trust the lane-status section.
- \`mg-create-web\` is currently measured as an MG-only lane because package-manager create flows are not semantically equivalent enough for a fair baseline.
- \`backend-*\` lanes are MG-only native runtime checks and use \`mg create-web -> mg install-web -> mg dev\` with real HTTP health probes.
- Re-run smoke mode with \`BENCH_MODE=smoke bash benchmark.sh\`.
- Re-run a subset with \`BENCH_LANES=cold-install,build BENCH_PMS=mg,bun bash benchmark.sh\`.
- Re-run full mode with \`BENCH_MODE=full BENCH_RUNS=10 BENCH_WARMUP=2 bash benchmark.sh\`.

## Output Files

- Markdown report: \`$(basename "$RESULTS_MD")\`
- Raw JSON timings: \`$(basename "$RESULTS_JSON")\`
- Lane status TSV: \`$(basename "$STATUS_TSV")\`
EOF
}

main() {
    resolve_selected_pms
    resolve_selected_lanes
    preflight
    verify_cli_surface
    create_fixtures
    write_runner
    append_report_header

    local lane
    local lane_pms=()
    for lane in "${SELECTED_LANES[@]}"; do
        if [[ "$lane" == heavy-* && "$ENABLE_HEAVY_PROFILE" != "1" ]]; then
            echo -e "$lane\tSKIPPED\theavy-profile-disabled" >>"$STATUS_TSV"
            continue
        fi

        lane_pms=("${SELECTED_PMS[@]}")
        if is_mg_only_lane "$lane"; then
            if contains_item "mg" "${SELECTED_PMS[@]}"; then
                lane_pms=(mg)
            else
                lane_pms=()
            fi
        fi

        if [[ ${#lane_pms[@]} -eq 0 ]]; then
            echo -e "$lane\tSKIPPED\tno-selected-pms" >>"$STATUS_TSV"
            continue
        fi

        run_lane "$(lane_title "$lane")" "$lane" "${lane_pms[@]}"
    done

    merge_json_results
    append_footer

    bold "=== BENCHMARK COMPLETE ==="
    green "Markdown: $RESULTS_MD"
    green "JSON:     $RESULTS_JSON"
    green "Status:   $STATUS_TSV"
    if [[ "$FAILURES" -gt 0 || "$INTERRUPTED" == "1" ]]; then
        exit 1
    fi
}

main "$@"
