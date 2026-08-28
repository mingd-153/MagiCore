#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: ./scripts/build.sh <package-name|bootstrap|all> [--target <triple>] [--profile <name>]"
  exit 1
fi

package="$1"
shift

case "$package" in
  bootstrap)
    cargo run -p mgc-dist -- build-bootstrap "$@"
    ;;
  all)
    cargo run -p mgc-dist -- build-all "$@"
    ;;
  magicore|magicore-web|magicore-ai|magicore-game|magicore-clo|magicore-cicd|magicore-iot|magicore-app|magicore-lib)
    cargo run -p mgc-dist -- build "$package" "$@"
    ;;
  *)
    echo "unsupported package: $package"
    exit 1
    ;;
esac
