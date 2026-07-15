#!/usr/bin/env bash
# Publish all MegaGate crates to crates.io in dependency order.
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
    sed -i '' 's|path = "core/crates/mg-types"|path = "core/crates/mg-types", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-crypto"|path = "core/crates/mg-crypto", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-http"|path = "core/crates/mg-http", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-config"|path = "core/crates/mg-config", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-fetcher"|path = "core/crates/mg-fetcher", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-ui"|path = "core/crates/mg-ui", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-lockfile"|path = "core/crates/mg-lockfile", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-resolver"|path = "core/crates/mg-resolver", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-store"|path = "core/crates/mg-store", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "core/crates/mg-adapter-base"|path = "core/crates/mg-adapter-base", version = "0.1.0"|' "$f"
    sed -i '' 's|path = "adapters/web"|path = "adapters/web", version = "0.1.0"|' "$f"
    echo "Added version fields to workspace path deps"
  fi
}

revert_versions() {
  git checkout -- "$ROOT/Cargo.toml"
  echo "Reverted Cargo.toml"
}

PUBLISH_ORDER=(
  mg-types mg-crypto mg-http mg-config mg-fetcher mg-ui
  mg-lockfile mg-resolver mg-store mg-adapter-base
  mg-web-adapter mg-dist mg
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

echo "Done! Install with: cargo install mg"
