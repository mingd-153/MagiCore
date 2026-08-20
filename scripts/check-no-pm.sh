#!/usr/bin/env bash
# L94: cấm gọi npm/npx/pnpm/yarn/bun trong code runtime (allowlist 00-index §5).
# Exception hợp lệ (không phải code path runtime):
#   - FORBIDDEN_TOOLS / allowlist.rs (định nghĩa lệnh cấm)
#   - adapters/web/benches/compare.rs (benchmark tham chiếu PM khác — không chạy trong product)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BAD=$(grep -rnE '\b(npm|npx|pnpm|yarn|bun)\b' cli/src adapters core --include='*.rs' \
  | grep -vE 'tests/|/tests|benches/|allowlist.rs|FORBIDDEN_TOOLS|npmrc|npm_registry|npm-format|npmjs' \
  | grep -E 'Command::new|run_allowlisted_tool|\.run\(|exec::{2}|process::Command' || true)

if [ -n "$BAD" ]; then
  echo "FORBIDDEN package-manager invocation found:"
  echo "$BAD"
  exit 1
fi
echo "OK: no npm/npx/pnpm/yarn/bun runtime invocations"
