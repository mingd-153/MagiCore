#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"
OUT="$LAB_DIR/security/security-$STAMP.md"

mkdir -p "$LAB_DIR/security"

count_or_zero() {
  local pattern="$1"
  local base="$2"
  local result
  result="$(rg -n -e "$pattern" "$base" 2>/dev/null | wc -l | tr -d ' ' || true)"
  echo "${result:-0}"
}

{
  echo "# Core-Web Security Lane"
  echo
  echo "- timestamp: $STAMP"
  echo
  echo "## Local checks"
  echo
  echo "- Cargo.lock present: $(test -f "$ROOT/Cargo.lock" && echo yes || echo no)"
  echo "- mg lockfile crate present: $(test -d "$ROOT/core/crates/mg-lockfile" && echo yes || echo no)"
  echo
  echo "## External scanners"
  echo
  if command -v gitleaks >/dev/null 2>&1; then
    echo "- gitleaks: installed"
  else
    echo "- gitleaks: not installed yet"
  fi
  if command -v trivy >/dev/null 2>&1; then
    echo "- trivy: installed"
  else
    echo "- trivy: not installed yet"
  fi
  if command -v syft >/dev/null 2>&1; then
    echo "- syft: installed"
  else
    echo "- syft: not installed yet"
  fi
  if command -v pip-audit >/dev/null 2>&1; then
    echo "- pip-audit: installed"
  else
    echo "- pip-audit: not installed yet"
  fi
  if command -v sonar-scanner >/dev/null 2>&1; then
    echo "- sonar-scanner: installed"
  else
    echo "- sonar-scanner: not installed yet"
  fi
  echo
  echo "## Quick heuristics"
  echo
  echo "Potential embedded tokens (simple pattern scan):"
  rg -n --hidden --glob '!.git' '(ghp_[A-Za-z0-9]{20,}|AIza[0-9A-Za-z_-]{20,}|sk_live_[0-9A-Za-z]{16,})' "$ROOT" || true
  echo
  echo "## Code heuristics for core-web surface"
  echo
  echo "- unsafe http references in core-web: $(count_or_zero 'http://' "$ROOT/adapters/web")"
  echo "- unwrap() usage in adapters/web: $(count_or_zero '\.unwrap\(' "$ROOT/adapters/web")"
  echo "- panic! usage in adapters/web: $(count_or_zero 'panic!\s*\(' "$ROOT/adapters/web")"
  echo "- TODO/FIXME in core-web scope: $(count_or_zero 'TODO|FIXME|HACK' "$ROOT/adapters/web")"
  echo "- production http references in lib.rs: $(count_or_zero 'http://' "$ROOT/adapters/web/src/lib.rs")"
  echo "- benchmark/test http references: $(count_or_zero 'http://' "$ROOT/adapters/web/src/bin" )"
  echo
  echo "### Direct matches"
  echo
  echo "HTTP references:"
  rg -n 'http://' "$ROOT/adapters/web" || true
  echo
  echo "panic! references:"
  rg -n 'panic!\s*\(' "$ROOT/adapters/web" || true
} > "$OUT"

echo "security lane summary: $OUT"
