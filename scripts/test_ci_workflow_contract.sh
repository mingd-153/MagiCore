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
grep -q '6323deb102c322ba6fcbdcafc7e3dddab59af2b6' "$ROOT/.github/workflows/ci.yml" || fail "CI rust-cache pin must resolve to v2.9.2 (latest, node24)"
grep -q '7b1c307e0dcbda6122208f10795a713336a9b35a' "$ROOT/.github/workflows/ci.yml" && fail "CI contains the broken Rust toolchain pin"
grep -q '6bed0761d98439e5a578e2877258200ad565ba87' "$ROOT/.github/workflows/ci.yml" || fail "CI Rust toolchain pin must resolve to stable"
grep -q '6bed0761d98439e5a578e2877258200ad565ba87' "$ALL_CORE" || fail "all-core Rust toolchain pin must resolve to stable"

# Verify GitHub Actions SHA pins (real commit refs)
checkout_sha='3d3c42e5aac5ba805825da76410c181273ba90b1' # v7.0.1 real
setup_node_sha='820762786026740c76f36085b0efc47a31fe5020' # v7.0.0 real
setup_python_sha='5fda3b95a4ea91299a34e894583c3862153e4b97' # v7.0.0 real
setup_go_sha='b7ad1dad31e06c5925ef5d2fc7ad053ef454303e' # v7.0.0 real

grep -q "actions/checkout@${checkout_sha}" "$ALL_CORE" || fail "all-core checkout SHA must be v7.0.1 real commit"
grep -q "actions/setup-node@${setup_node_sha}" "$ALL_CORE" || fail "all-core setup-node SHA must be v7.0.0 real commit"
grep -q "actions/setup-python@${setup_python_sha}" "$ALL_CORE" || fail "all-core setup-python SHA must be v7.0.0 real commit"
grep -q "actions/setup-go@${setup_go_sha}" "$ALL_CORE" || fail "all-core setup-go SHA must be v7.0.0 real commit"

# Cross-check pinned SHAs against the REAL upstream tag refs via git
# ls-remote — a self-fulfilling hardcoded SHA list proves nothing, so the
# contract queries GitHub and fails closed on mismatch. Offline runs skip
# with a loud warning instead of silently passing.
# Đối chiếu SHA pin với tag THẬT trên GitHub qua git ls-remote — danh sách
# hardcode tự trỏ vào chính nó không chứng minh gì; contract fail-closed
# khi lệch. Môi trường offline bỏ qua với cảnh báo, không âm thầm pass.
verify_pin() {
    local repo="$1" tag="$2" expected="$3"
    local actual
    # Annotated tags point at a tag object; peel to the commit with ^{}.
    # Tag annotated trỏ tới tag object; lột bằng ^{} để lấy commit thật.
    # `set -e` also applies inside command substitutions. Preserve the intended
    # offline-warning behavior instead of aborting before the empty-result check.
    # `set -e` cũng áp dụng trong command substitution; giữ hành vi cảnh báo
    # offline thay vì dừng trước khi kiểm tra kết quả rỗng.
    actual="$(git ls-remote "https://github.com/${repo}.git" "refs/tags/${tag}^{}" 2>/dev/null | awk '{print $1}' || true)"
    if [ -z "$actual" ]; then
        actual="$(git ls-remote "https://github.com/${repo}.git" "refs/tags/${tag}" 2>/dev/null | awk '{print $1}' || true)"
    fi
    if [ -z "$actual" ]; then
        echo "WARN: cannot reach github.com to verify ${repo}@${tag} — skipping remote verification (offline?)" >&2
        return 0
    fi
    if [ "$actual" != "$expected" ]; then
        fail "${repo}@${tag} pin ${expected} does not match upstream commit ${actual}"
    fi
}
if command -v git >/dev/null 2>&1; then
    verify_pin actions/checkout v7.0.1 "$checkout_sha"
    verify_pin actions/setup-node v7.0.0 "$setup_node_sha"
    verify_pin actions/setup-python v7.0.0 "$setup_python_sha"
    verify_pin actions/setup-go v7.0.0 "$setup_go_sha"
    verify_pin Swatinem/rust-cache v2.9.2 '6323deb102c322ba6fcbdcafc7e3dddab59af2b6'
    verify_pin actions/upload-artifact v7.0.1 '043fb46d1a93c77aae656e7c1c64a875d1fc6a0a'
    verify_pin actions/download-artifact v8.0.1 '3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c'
    verify_pin astral-sh/setup-uv v10.0.1 '20cfd1bf945f4377ade1205e4dbc17946fc9a30d'
    verify_pin softprops/action-gh-release v3.0.3 'efb35369e0ad2afab669f228072c1b0d510eae64'
    verify_pin actions/attest-build-provenance v4.2.2 '4d101475d8b20a2381f78447822ac1eab6504dd8'
    verify_pin subosito/flutter-action v2.16.0 '44ac965b96f18d999802d4b807e3256d5a3f9fa1'
else
    echo "WARN: git not available — skipping remote SHA verification" >&2
fi

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

grep -q 'node-version: "24"' "$ALL_CORE" || fail "Node.js lifecycle pin must be 24 (latest T9-2026)"
grep -q 'python-version: "3.14"' "$ALL_CORE" || fail "Python lifecycle pin is stale"
grep -q 'flutter-version: "3.47.2"' "$ALL_CORE" || fail "Flutter lifecycle pin is stale"
grep -q 'version: "0.12.10"' "$ALL_CORE" || fail "uv lifecycle pin is stale or floating"

app_matrix="$(sed -n '/^  app-lifecycle:/,/^  lib-lifecycle:/p' "$ALL_CORE")"
grep -q 'windows-latest' <<<"$app_matrix" || fail "App lifecycle must cover Windows"

web_section="$(sed -n '/^  web-lifecycle:/,/^  ai-lifecycle:/p' "$ALL_CORE")"
ai_section="$(sed -n '/^  ai-lifecycle:/,/^  app-lifecycle:/p' "$ALL_CORE")"
app_section="$(sed -n '/^  app-lifecycle:/,/^  lib-lifecycle:/p' "$ALL_CORE")"
lib_section="$(sed -n '/^  lib-lifecycle:/,/^  lifecycle-summary:/p' "$ALL_CORE")"
rust_setup="$(grep -A1 -- '- name: Setup Rust' <<<"$lib_section")"
grep -q 'if:' <<<"$rust_setup" && fail "Rust setup cannot be conditional because every lib row builds mgc"
grep -q '"\$MGC_PARENT_BIN" build' <<<"$lib_section" || fail "Lib lifecycle must build through the OS-correct mgc binary"

grep -q '"\$MGC_PARENT_BIN" test' <<<"$ai_section" || fail "AI lifecycle must test through the OS-correct mgc binary"
grep -q '"\$MGC_PARENT_BIN" build' <<<"$ai_section" || fail "AI lifecycle must build through the OS-correct mgc binary"

for section in "$web_section" "$ai_section" "$app_section" "$lib_section"; do
  grep -q 'MGC_BIN=./target/release/mgc.exe' <<<"$section" \
    || fail "every core lifecycle job must resolve mgc.exe on Windows"
done

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
