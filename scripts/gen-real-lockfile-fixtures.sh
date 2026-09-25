#!/usr/bin/env bash
# Regenerate the REAL-lockfile fixtures for mgc-lockfile import tests.
# Sinh lại fixture lockfile THẬT cho test import của mgc-lockfile.
#
# P0 finding #9 (2026-09-12): hand-written fixtures pass even when the
# importer drifts from what real writers emit. These fixtures are
# produced by the PINNED tools below and committed; when the tool
# versions change, regenerate and review the diff — the test then
# catches writer-shape drift.
# P0 finding #9: fixture viết tay pass ngay cả khi importer trôi khỏi
# output thật của writer. Fixture này do CÔNG CỤ GHIM bên dưới sinh ra
# và được commit; khi đổi version công cụ, sinh lại và review diff —
# test sẽ bắt được sự trôi của writer.
#
# Usage: scripts/gen-real-lockfile-fixtures.sh <output-dir>
# Requires: bun 1.3.14, deno 2.9.3 (pinned below — do not bump silently)
set -euo pipefail

BUN_VERSION="1.3.14"   # pinned — ghim
DENO_VERSION="2.9.3"   # pinned — ghim
OUT="${1:?usage: gen-real-lockfile-fixtures.sh <output-dir>}"

mkdir -p "$OUT"

# --- bun.lock: real `bun install` output (JSONC writer, v1 schema) ---
WORK="$(mktemp -d)"
cat > "$WORK/package.json" <<'JSON'
{
  "name": "mgc-fixture",
  "dependencies": {
    "ms": "^2.1.3"
  },
  "devDependencies": {
    "left-pad": "^1.3.0"
  }
}
JSON
(cd "$WORK" && bun install --silent)
cp "$WORK/bun.lock" "$OUT/bun.lock"

# --- deno.lock: real `deno cache` output (v5 schema, jsr + npm pins) ---
WORK="$(mktemp -d)"
cat > "$WORK/deno.json" <<'JSON'
{
  "imports": {
    "lodash": "npm:lodash@4.17.20",
    "@std/bytes": "jsr:@std/bytes@0.224.0"
  }
}
JSON
cat > "$WORK/main.ts" <<'TS'
import lodash from "lodash";
import { concat } from "@std/bytes";
console.log(lodash.VERSION, concat(new Uint8Array([1]), new Uint8Array([2])).length);
TS
(cd "$WORK" && deno cache main.ts)
cp "$WORK/deno.lock" "$OUT/deno.lock"

echo "fixtures written to $OUT (bun $BUN_VERSION, deno $DENO_VERSION)"
