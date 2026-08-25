#!/usr/bin/env bash
# Publish all MagiCore crates to crates.io in dependency order.
# Usage: ./scripts/publish.sh [--dry-run] [--token CARGO_REGISTRY_TOKEN]
set -euo pipefail

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then DRY_RUN=true; shift; fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Add version to workspace path deps so cargo publish can resolve them
# ponytail: sed one-liner, revert after publish
add_versions() {
  local f="$ROOT/Cargo.toml"
  # Only add if not already there
  if ! grep -q 'version = "0.1.0"' "$f" 2>/dev/null; then
    sed -i '' 's|path = "core/crates/mgc-types"|path = "core/crates/mgc-types", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-crypto"|path = "core/crates/mgc-crypto", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-http"|path = "core/crates/mgc-http", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-config"|path = "core/crates/mgc-config", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-fetcher"|path = "core/crates/mgc-fetcher", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-ui"|path = "core/crates/mgc-ui", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-lockfile"|path = "core/crates/mgc-lockfile", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-resolver"|path = "core/crates/mgc-resolver", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-store"|path = "core/crates/mgc-store", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mgc-adapter-base"|path = "core/crates/mgc-adapter-base", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "adapters/web"|path = "adapters/web", version = "0.1.0"|' "$f"
    echo "Added version fields to workspace path deps"
  fi
}

revert_versions() {
  git checkout -- "$ROOT/Cargo.toml"
  echo "Reverted Cargo.toml"
}

PUBLISH_ORDER=(
  mgc-types mgc-crypto mgc-http mgc-config mgc-fetcher mgc-ui
  mgc-lockfile mgc-resolver mgc-store mgc-adapter-base
  mgc-web-adapter mgc-dist mgc
)

publish_crate() {
  local name="$1"
  echo "── Publishing $name ──"
  if $DRY_RUN; then
    (cd "$ROOT" && cargo publish -p "$name" --dry-run --allow-dirty 2>&1 | tail -5)
  else
    (cd "$ROOT" && cargo publish -p "$name" 2>&1 | tail -5)
  fi
  echo ""
}

add_versions

for crate in "${PUBLISH_ORDER[@]}"; do
  publish_crate "$crate"
done

revert_versions

echo "Done! Install with: cargo install mgc"
