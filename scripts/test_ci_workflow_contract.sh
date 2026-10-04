#!/usr/bin/env bash
# Enforce fail-closed CI/release invariants. — Ép các invariant CI/release fail-closed.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ALL_CORE="$ROOT/.github/workflows/delegated-compatibility-matrix.yml"
RELEASE="$ROOT/.github/workflows/release.yml"
SECURITY="$ROOT/.github/workflows/security.yml"
VERIFY_DISTRIBUTION="$ROOT/.github/workflows/verify-release-distribution.yml"
LOCAL_RUNNER="$ROOT/scripts/run_all_core_tests.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

grep -q 'run: bash scripts/test_ci_workflow_contract.sh' "$ROOT/.github/workflows/ci.yml" \
  || fail "CI must execute this contract before its release and action-pin assertions are treated as gates"
grep -Fq 'python3 "$ROOT/scripts/audit_workflow_action_pins.py"' "$ROOT/scripts/test_ci_workflow_contract.sh" \
  || fail "the CI-invoked workflow contract must audit every workflow action reference"
python3 "$ROOT/scripts/test_workflow_action_pins.py" || fail "workflow action-pin audit regression tests failed"
python3 "$ROOT/scripts/audit_workflow_action_pins.py" "$ROOT/.github/workflows" \
  || fail "a workflow action reference is not immutably pinned"

grep -q '^  pull_request:' "$ALL_CORE" || fail "delegated-compatibility matrix must run on pull requests"
grep -q '"core/\*\*"' "$ALL_CORE" || fail "delegated-compatibility matrix path filter misses the core directory"
grep -q '"fix/\*\*"' "$ALL_CORE" || fail "delegated-compatibility matrix must run on RC fix branches"
grep -q '"fix/\*\*"' "$ROOT/.github/workflows/ci.yml" || fail "CI must run on RC fix branches"
grep -q '".github/workflows/security.yml"' "$SECURITY" || fail "security workflow changes must retrigger security checks"
grep -q '82a92a6e8fbeee089604da2575dc567ae9ddeaff' "$ROOT/.github/workflows/ci.yml" && fail "CI contains the invalid rust-cache SHA"
grep -q '6323deb102c322ba6fcbdcafc7e3dddab59af2b6' "$ROOT/.github/workflows/ci.yml" || fail "CI rust-cache pin must resolve to v2.9.2 (latest, node24)"
grep -q '7b1c307e0dcbda6122208f10795a713336a9b35a' "$ROOT/.github/workflows/ci.yml" && fail "CI contains the broken Rust toolchain pin"
grep -q '6bed0761d98439e5a578e2877258200ad565ba87' "$ROOT/.github/workflows/ci.yml" || fail "CI Rust toolchain pin must resolve to stable"
grep -q '6bed0761d98439e5a578e2877258200ad565ba87' "$ALL_CORE" || fail "delegated-compatibility Rust toolchain pin must resolve to stable"
grep -q 'python3 scripts/test_audit_dependency_delegation.py' "$ROOT/.github/workflows/ci.yml" || fail "CI must run dependency ownership audit negative controls"
grep -q 'python3 scripts/test_audit_capability_matrix.py' "$ROOT/.github/workflows/ci.yml" || fail "CI must test audit-matrix source provenance"
grep -q 'working_tree_clean.*is not True' "$RELEASE" || fail "release audit gate must reject dirty-tree matrices"
grep -q 'working_tree_clean.*is not True' "$ROOT/.github/workflows/nightly-audit-matrix.yml" || fail "nightly audit gate must reject dirty-tree matrices"
grep -q 'scripts/test_dep_gate_consistency.py' "$ROOT/.github/workflows/lifecycle-matrix.yml" || fail "lifecycle CI must test binary-to-matrix operation ownership"
grep -q 'CI cannot reach github.com' "$ROOT/scripts/verify_github_action_pin.sh" || fail "CI must fail closed when upstream action pins cannot be verified"
grep -q 'CI cannot verify upstream action pins without git' "$ROOT/scripts/verify_github_action_pin.sh" || fail "CI must fail closed when git is unavailable for action-pin verification"
grep -q 'UNVERIFIED: cannot reach github.com' "$ROOT/scripts/verify_github_action_pin.sh" || fail "offline action-pin checks must be reported as unverified"
python3 "$ROOT/scripts/test_github_action_pin.py" || fail "action-pin verifier behavior tests failed"

cross_rev='64b5bb4d3d34de062552b9a2093affe77b4ad16a'
cross_install="cargo install cross --git https://github.com/cross-rs/cross --rev ${cross_rev} --locked"
grep -Fq "$cross_install" "$RELEASE" \
  || fail "release cross compiler install must use the reviewed immutable revision and Cargo.lock"
grep -Fq "$cross_install" "$ROOT/scripts/build_all_platforms.sh" \
  || fail "local cross build instructions must use the same immutable revision as release CI"

manifest_signing="$(sed -n '/^      # Release provenance (P0)/,/^      # Note: Attestations/p' "$RELEASE")"
grep -q 'MGC_RELEASE_REQUIRE_SIGNED: "1"' <<<"$manifest_signing" \
  || fail "release manifest signing must require a signature"
if grep -Eiq 'without the secret.*(publishes|publish).*unsigned|publishes an unsigned manifest' <<<"$manifest_signing"; then
  fail "release workflow must not claim that unsigned manifests can be published when signing is mandatory"
fi

signed_distribution="$(sed -n '/^  verify-signed-manifest:/,$p' "$ROOT/.github/workflows/verify-release-distribution.yml")"
grep -q 'python3 scripts/verify-release-manifest-signature.py' <<<"$signed_distribution" \
  || fail "distribution verification must run the tested native signature-and-archive verifier"
grep -q 'MGC_RELEASE_SIGNING_KEY:.*secrets.MGC_RELEASE_SIGNING_KEY' <<<"$signed_distribution" \
  || fail "distribution verification must require the configured release trust key"
grep -q 'contents: write' <<<"$signed_distribution" \
  || fail "signed distribution verification must be able to read draft release assets before promotion"
if grep -Eiq 'pip[[:space:]]+install.*cryptography|signature NOT verified|gh release download.*\|\|[[:space:]]*true' <<<"$signed_distribution"; then
  fail "signed distribution verification must not install an external verifier or waive missing evidence"
fi
grep -q 'magicore-\*\.tar.gz' <<<"$signed_distribution" \
  || fail "signed distribution verification must download every tar.gz archive for digest binding"
grep -q 'magicore-\*\.zip' <<<"$signed_distribution" \
  || fail "signed distribution verification must download every zip archive for digest binding"
grep -q 'every signed archive must be downloaded and verified' "$ROOT/scripts/verify-release-manifest-signature.py" \
  || fail "signed distribution verifier must reject omitted manifest-bound archives"
grep -q 'missing required release variants' "$ROOT/scripts/verify-release-manifest-signature.py" \
  || fail "signed distribution verifier must enforce required cross-platform variants"

# Verify each published architecture, including Apple Silicon; select exactly
# one all-core archive so the web-only match cannot make ARCHIVE ambiguous.
# Kiểm từng kiến trúc phát hành, gồm Apple Silicon; chọn đúng archive all-core.
archive_verification="$(sed -n '/^  verify-archive-download:/,/^  verify-homebrew:/p' "$VERIFY_DISTRIBUTION")"
grep -q 'arch: arm64' <<<"$archive_verification" \
  || fail "public distribution verification must exercise the published macOS arm64 archive"
grep -A3 '^          - os: ubuntu-24.04-arm$' <<<"$archive_verification" | grep -q 'arch: arm64' \
  || fail "public distribution verification must exercise the published Linux arm64 archive on a native runner"
grep -q 'arch: x64' <<<"$archive_verification" \
  || fail "public distribution verification must retain x64 archive coverage"
macos_intel_target="$(grep -A2 '^          - os: macos-15-intel$' <<<"$archive_verification")"
grep -q 'arch: x64' <<<"$macos_intel_target" \
  || fail "the Intel macOS runner must verify the x64 archive"
macos_arm_target="$(grep -A2 '^          - os: macos-latest$' <<<"$archive_verification")"
grep -q 'arch: arm64' <<<"$macos_arm_target" \
  || fail "the Apple Silicon macOS runner must verify the arm64 archive"
grep -q 'EXT="zip"' <<<"$archive_verification" \
  || fail "Windows verification must select the zip archive format"
grep -q 'EXT="tar.gz"' <<<"$archive_verification" \
  || fail "Unix verification must select the tar.gz archive format"
grep -q 'ARCHIVE="magicore-${VERSION}-${OS_NAME}-${ARCH}.${EXT}"' <<<"$archive_verification" \
  || fail "verification must select exactly one all-core archive for its OS and architecture"
if grep -Eq 'ARCHIVE=.*ls magicore-[*]' <<<"$archive_verification"; then
  fail "distribution verification must not select ambiguous all-core and web-only archive matches"
fi

# The Linux ARM64 target must build and smoke-test on the native ARM runner.
# Target Linux ARM64 phải được build và smoke test trên runner ARM bản địa.
release_build_matrix="$(sed -n '/^  build-release:/,/^  test-release-artifacts:/p' "$RELEASE")"
linux_arm_build="$(grep -A6 '^          - os: ubuntu-24.04-arm$' <<<"$release_build_matrix" | head -n 7)"
grep -q 'target: aarch64-unknown-linux-gnu' <<<"$linux_arm_build" \
  || fail "release build matrix must target native Linux ARM64"
grep -q 'arch: arm64' <<<"$linux_arm_build" \
  || fail "Linux ARM64 release archive must carry the arm64 contract identity"
grep -q 'use_cross: false' <<<"$linux_arm_build" \
  || fail "Linux ARM64 release target must build natively on its hosted runner"
release_smoke_matrix="$(sed -n '/^  test-release-artifacts:/,/^  pre-publish-verification:/p' "$RELEASE")"
[[ "$(grep -c '^          - os: ubuntu-24.04-arm$' <<<"$release_smoke_matrix")" -eq 2 ]] \
  || fail "all-core and web Linux ARM64 artifacts must both have smoke-test rows"
linux_arm_smoke_blocks="$(grep -A3 '^          - os: ubuntu-24.04-arm$' <<<"$release_smoke_matrix")"
[[ "$(grep -c 'arch: arm64' <<<"$linux_arm_smoke_blocks")" -eq 2 ]] \
  || fail "Linux ARM64 smoke-test rows must use the matching architecture identity"

# Verify GitHub Actions SHA pins (real commit refs)
checkout_sha='3d3c42e5aac5ba805825da76410c181273ba90b1' # v7.0.1 real
setup_node_sha='820762786026740c76f36085b0efc47a31fe5020' # v7.0.0 real
setup_python_sha='5fda3b95a4ea91299a34e894583c3862153e4b97' # v7.0.0 real
setup_go_sha='b7ad1dad31e06c5925ef5d2fc7ad053ef454303e' # v7.0.0 real

grep -q "actions/checkout@${checkout_sha}" "$ALL_CORE" || fail "delegated-compatibility checkout SHA must be v7.0.1 real commit"
grep -q "actions/setup-node@${setup_node_sha}" "$ALL_CORE" || fail "delegated-compatibility setup-node SHA must be v7.0.0 real commit"
grep -q "actions/setup-python@${setup_python_sha}" "$ALL_CORE" || fail "delegated-compatibility setup-python SHA must be v7.0.0 real commit"
grep -q "actions/setup-go@${setup_go_sha}" "$ALL_CORE" || fail "delegated-compatibility setup-go SHA must be v7.0.0 real commit"

source "$ROOT/scripts/verify_github_action_pin.sh"
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
    if [[ "${CI:-}" == "true" || "${CI:-}" == "1" || "${GITHUB_ACTIONS:-}" == "true" ]]; then
        fail "CI cannot verify upstream action pins without git"
    fi
    echo "UNVERIFIED: git is unavailable; upstream action SHA checks cannot pass" >&2
    exit 2
fi

setup_go_count="$(grep -c "actions/setup-go@${setup_go_sha}" "$ALL_CORE")"
[[ "$setup_go_count" -eq 4 ]] || fail "delegated-compatibility matrix must provision pinned Go for all four core jobs"
go_version_count="$(grep -c 'go-version: "1.27.1"' "$ALL_CORE")"
[[ "$go_version_count" -eq 4 ]] || fail "delegated-compatibility matrix must pin Go 1.27.1 for all four core jobs"
grep -q "actions/setup-go@${setup_go_sha}" "$RELEASE" || fail "release builds must provision pinned Go for esbuild-rs"
grep -q 'go-version: "1.27.1"' "$RELEASE" || fail "release builds must pin Go 1.27.1"
go_version_values=$(grep -rhoE 'go-version: "[^"]+"' "$ROOT/.github/workflows" | sort -u)
[[ "$go_version_values" == 'go-version: "1.27.1"' ]] || fail "all workflow Go toolchains must use the canonical Go 1.27.1 pin; found: ${go_version_values//$'\n'/, }"
go_archive_values=$(grep -oE 'go[0-9]+\.[0-9]+\.[0-9]+\.linux-amd64\.tar\.gz' "$RELEASE" | sort -u)
[[ "$go_archive_values" == 'go1.27.1.linux-amd64.tar.gz' ]] || fail "release scanner archive must match canonical Go 1.27.1; found: ${go_archive_values//$'\n'/, }"
grep -q 'GO_SHA256="63d339f0da5ab53635a56f2490a7984dfe12dfcff22ad749f63edaf590168445"' "$RELEASE" \
  || fail "release scanner must pin the official Go 1.27.1 Linux AMD64 archive checksum"
go_sha_check_line=$(grep -n -m1 'sha256sum --check --strict -' "$RELEASE" | cut -d: -f1 || true)
go_extract_line=$(grep -n -m1 'sudo tar -C /usr/local -xzf /tmp/go.tgz' "$RELEASE" | cut -d: -f1 || true)
[[ -n "$go_sha_check_line" && -n "$go_extract_line" && "$go_sha_check_line" -lt "$go_extract_line" ]] \
  || fail "release scanner archive must pass SHA-256 verification before extraction"

if grep -Eq 'uses: [^ ]+@(v[0-9]+|main|master|stable|latest)([[:space:]]|$)' "$ALL_CORE"; then
  fail "delegated-compatibility workflow contains floating action references"
fi

grep -q 'App: SKIP' "$LOCAL_RUNNER" && fail "local all-core runner must not convert App failures into skips"
grep -q 'ALL CORES LIFECYCLE VERIFIED' "$LOCAL_RUNNER" || fail "local runner summary missing"
grep -q 'publish=true' "$RELEASE" && fail "release instructions reference the removed publish input"

release_dry_run=$(sed -n '/^  dry-run-summary:/,$p' "$RELEASE")
grep -q 'Ready to publish: YES' <<<"$release_dry_run" && fail "release dry-run must not claim publish readiness without the tag-triggered publish gates"
grep -q 'Publish readiness: NOT ASSESSED (dry-run does not authorize publishing)' <<<"$release_dry_run" \
  || fail "release dry-run must state that it does not authorize publishing"

grep -q 'node-version: "24"' "$ALL_CORE" || fail "Node.js lifecycle pin must be 24 (latest T9-2026)"
grep -q 'python-version: "3.14"' "$ALL_CORE" || fail "Python lifecycle pin is stale"
grep -q 'flutter-version: "3.47.4"' "$ALL_CORE" || fail "Flutter lifecycle pin is stale"
grep -Eq 'cargo test -p mgc --test full_lifecycle_e2e --locked -- .*--ignored' "$ROOT/.github/workflows/ci.yml" \
  || fail "CI provisions pytest and Flutter but does not execute the ignored AI/App full lifecycle tests"
if grep -vE '^[[:space:]]*#' "$ALL_CORE" | grep -Eq 'setup-uv|(^|[[:space:]])uv([[:space:]]|$)'; then
  fail "native AI lifecycle matrix must not provision or invoke uv"
fi

app_matrix="$(sed -n '/^  app-lifecycle:/,/^  lib-lifecycle:/p' "$ALL_CORE")"
grep -q 'windows-latest' <<<"$app_matrix" || fail "App lifecycle must cover Windows"

web_section="$(sed -n '/^  web-lifecycle:/,/^  ai-lifecycle:/p' "$ALL_CORE")"
ai_section="$(sed -n '/^  ai-lifecycle:/,/^  app-lifecycle:/p' "$ALL_CORE")"
app_section="$(sed -n '/^  app-lifecycle:/,/^  lib-lifecycle:/p' "$ALL_CORE")"
lib_section="$(sed -n '/^  lib-lifecycle:/,/^  lifecycle-summary:/p' "$ALL_CORE")"
rust_setup="$(grep -A1 -- '- name: Setup Rust' <<<"$lib_section")"
grep -q 'if:' <<<"$rust_setup" && fail "Rust setup cannot be conditional because every lib row builds mgc"
grep -q '"\$MGC_PARENT_BIN" build' <<<"$lib_section" || fail "Lib lifecycle must build through the OS-correct mgc binary"

grep -q '"\$MGC_PARENT_BIN" add-ai six@1.17.0' <<<"$ai_section" || fail "AI lifecycle must add a real package through the native mgc path"
grep -q '"\$MGC_PARENT_BIN" install' <<<"$ai_section" || fail "AI lifecycle must install the native lock through mgc"
grep -q 'python -m py_compile' <<<"$ai_section" || fail "AI lifecycle must syntax-check the scaffold with the provisioned Python runtime"

for section in "$web_section" "$ai_section" "$app_section" "$lib_section"; do
  grep -q 'MGC_BIN=./target/release/mgc.exe' <<<"$section" \
    || fail "every core lifecycle job must resolve mgc.exe on Windows"
done

if grep -Eq '\|\|[[:space:]]*(true|echo)|exit[[:space:]]+0[[:space:]]*#.*skip' "$ALL_CORE"; then
  fail "delegated-compatibility matrix contains a pass-through bypass"
fi

# Anti-overclaim: the workflow verifies Web/AI/App/Lib only. Its name and
# summary must not claim all 9 cores until Game/IoT/Hardware/Cloud/CI-CD
# lifecycle jobs actually exist in the matrix.
# Chống overclaim: workflow chỉ verify Web/AI/App/Lib — tên và summary
# không được claim 9 core trước khi job lifecycle của các core kia tồn tại.
if grep -Eq '^name:[[:space:]]+All-Core' "$ALL_CORE"; then
  fail "workflow name overclaims: only Web/AI/App/Lib have lifecycle jobs — keep the Delegated Compatibility name or add all 9 cores"
fi
if grep -q 'ALL CORES' "$ALL_CORE"; then
  fail "workflow summary overclaims 'ALL CORES' while covering only four cores"
fi

checkout_line="$(grep -n 'name: Checkout repository' "$RELEASE" | head -1 | cut -d: -f1)"
contract_line="$(grep -n 'name: Set artifact names' "$RELEASE" | head -1 | cut -d: -f1)"
[[ "$checkout_line" -lt "$contract_line" ]] || fail "release contract runs before checkout"

ref_count="$(grep -c 'GITHUB_REF_NAME' "$RELEASE" || true)"
[[ "$ref_count" -eq 1 ]] || fail "resolved release version must be reused downstream"

grep -q '10 SBOMs' "$RELEASE" || fail "release summary must require all ten SBOMs (5 platform pairs x all/web)"
grep -q 'Windows doesn.t generate SBOM' "$RELEASE" && fail "Windows SBOM must not be silently excluded"

grep -q 'cargo-audit --version 0.22.2' "$SECURITY" || fail "cargo-audit pin is stale"
grep -q 'OSV_VERSION="2.5.1"' "$SECURITY" || fail "OSV-Scanner pin is stale"

# T0.5 installer freshness: the embedded SHA-256 in dev/cicd.rs must be
# non-empty AND must match scripts/install-from-gh.sh at the pinned
# commit — a stale checksum fails the pipeline it generates.
# (Portable sed: BSD grep on macOS runners has no -P.)
# (Tươi checksum installer: SHA-256 nhúng phải khác rỗng VÀ khớp file ở
# commit ghim — checksum cũ làm fail pipeline nó sinh ra.)
installer_sha="$(sed -n 's/.*MGC_INSTALLER_SHA: &str = "\([0-9a-f][0-9a-f]*\)".*/\1/p' "$ROOT/cli/src/commands/core/dev/cicd.rs" | head -n 1)"
[[ -n "$installer_sha" ]] || fail "installer commit pin is missing"
embedded_sha256="$(grep -A1 'MGC_INSTALLER_SHA256: &str' "$ROOT/cli/src/commands/core/dev/cicd.rs" | sed -n 's/.*"\([0-9a-f][0-9a-f]*\)".*/\1/p' | head -n 1)"
[[ -n "$embedded_sha256" ]] || fail "installer checksum is not pinned (MGC_INSTALLER_SHA256 empty)"
expected_sha256=$(git -C "$ROOT" show "${installer_sha}:scripts/install-from-gh.sh" | shasum -a 256 | awk '{print $1}')
[[ "$embedded_sha256" == "$expected_sha256" ]] || fail "installer checksum is stale for the pinned commit"

echo "PASS: CI workflow contract"
