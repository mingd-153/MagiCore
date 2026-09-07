#!/usr/bin/env bash
# Enforce fail-closed CI/release invariants. — Ép các invariant CI/release fail-closed.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ALL_CORE="$ROOT/.github/workflows/all-core-lifecycle.yml"
RELEASE="$ROOT/.github/workflows/release.yml"
SECURITY="$ROOT/.github/workflows/security.yml"
LOCAL_RUNNER="$ROOT/scripts/run_all_core_tests.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

grep -q '^  pull_request:' "$ALL_CORE" || fail "all-core lifecycle must run on pull requests"
grep -q '"core/\*\*"' "$ALL_CORE" || fail "all-core lifecycle path filter misses the core directory"
grep -q '"fix/\*\*"' "$ALL_CORE" || fail "all-core lifecycle must run on RC fix branches"
grep -q '"fix/\*\*"' "$ROOT/.github/workflows/ci.yml" || fail "CI must run on RC fix branches"
grep -q '".github/workflows/security.yml"' "$SECURITY" || fail "security workflow changes must retrigger security checks"
grep -q '82a92a6e8fbeee089604da2575dc567ae9ddeaff' "$ROOT/.github/workflows/ci.yml" && fail "CI contains the invalid rust-cache SHA"
grep -q '82a92a6e8fbeee089604da2575dc567ae9ddeaab' "$ROOT/.github/workflows/ci.yml" || fail "CI rust-cache pin must resolve to v2.7.5"
grep -q '7b1c307e0dcbda6122208f10795a713336a9b35a' "$ROOT/.github/workflows/ci.yml" && fail "CI contains the broken Rust toolchain pin"
grep -q '6bed0761d98439e5a578e2877258200ad565ba87' "$ROOT/.github/workflows/ci.yml" || fail "CI Rust toolchain pin must resolve to stable"
grep -q '6bed0761d98439e5a578e2877258200ad565ba87' "$ALL_CORE" || fail "all-core Rust toolchain pin must resolve to stable"

setup_go_sha='924ae3a1cded613372ab5595356fb5720e22ba16'
setup_go_count="$(grep -c "actions/setup-go@${setup_go_sha}" "$ALL_CORE")"
[[ "$setup_go_count" -eq 4 ]] || fail "all-core lifecycle must provision pinned Go for all four core jobs"
go_version_count="$(grep -c 'go-version: "1.27.1"' "$ALL_CORE")"
[[ "$go_version_count" -eq 4 ]] || fail "all-core lifecycle must pin Go 1.27.1 for all four core jobs"
grep -q "actions/setup-go@${setup_go_sha}" "$RELEASE" || fail "release builds must provision pinned Go for esbuild-rs"
grep -q 'go-version: "1.27.1"' "$RELEASE" || fail "release builds must pin Go 1.27.1"

if grep -Eq 'uses: [^ ]+@(v[0-9]+|main|master|stable|latest)([[:space:]]|$)' "$ALL_CORE"; then
  fail "all-core workflow contains floating action references"
fi

grep -q 'App: SKIP' "$LOCAL_RUNNER" && fail "local all-core runner must not convert App failures into skips"
grep -q 'ALL CORES LIFECYCLE VERIFIED' "$LOCAL_RUNNER" || fail "local runner summary missing"
grep -q 'publish=true' "$RELEASE" && fail "release instructions reference the removed publish input"

grep -q 'node-version: "22"' "$ALL_CORE" || fail "Node.js lifecycle pin is stale (must be 22 LTS — node 24 CSPRNG crash on GH Windows runners)"
grep -q 'python-version: "3.14"' "$ALL_CORE" || fail "Python lifecycle pin is stale"
grep -q 'flutter-version: "3.47.2"' "$ALL_CORE" || fail "Flutter lifecycle pin is stale"
grep -q 'version: "0.12.10"' "$ALL_CORE" || fail "uv lifecycle pin is stale or floating"

app_matrix="$(sed -n '/^  app-lifecycle:/,/^  lib-lifecycle:/p' "$ALL_CORE")"
grep -q 'windows-latest' <<<"$app_matrix" || fail "App lifecycle must cover Windows"

lib_section="$(sed -n '/^  lib-lifecycle:/,/^  lifecycle-summary:/p' "$ALL_CORE")"
rust_setup="$(grep -A1 -- '- name: Setup Rust' <<<"$lib_section")"
grep -q 'if:' <<<"$rust_setup" && fail "Rust setup cannot be conditional because every lib row builds mgc"
grep -q '../target/release/mgc build' <<<"$lib_section" || fail "Lib lifecycle must build through mgc"

ai_section="$(sed -n '/^  ai-lifecycle:/,/^  app-lifecycle:/p' "$ALL_CORE")"
grep -q '../target/release/mgc test' <<<"$ai_section" || fail "AI lifecycle must test through mgc"
grep -q '../target/release/mgc build' <<<"$ai_section" || fail "AI lifecycle must build through mgc"

if grep -Eq '\|\|[[:space:]]*(true|echo)|exit[[:space:]]+0[[:space:]]*#.*skip' "$ALL_CORE"; then
  fail "all-core lifecycle contains a pass-through bypass"
fi

checkout_line="$(grep -n 'name: Checkout repository' "$RELEASE" | head -1 | cut -d: -f1)"
contract_line="$(grep -n 'name: Set artifact names' "$RELEASE" | head -1 | cut -d: -f1)"
[[ "$checkout_line" -lt "$contract_line" ]] || fail "release contract runs before checkout"

ref_count="$(grep -c 'GITHUB_REF_NAME' "$RELEASE" || true)"
[[ "$ref_count" -eq 1 ]] || fail "resolved release version must be reused downstream"

grep -q '6 SBOMs' "$RELEASE" || fail "release summary must require all six SBOMs"
grep -q 'Windows doesn.t generate SBOM' "$RELEASE" && fail "Windows SBOM must not be silently excluded"

grep -q 'cargo-audit --version 0.22.2' "$SECURITY" || fail "cargo-audit pin is stale"
grep -q 'OSV_VERSION="2.5.1"' "$SECURITY" || fail "OSV-Scanner pin is stale"

echo "PASS: CI workflow contract"
