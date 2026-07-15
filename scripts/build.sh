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
    cargo run -p mg-dist -- build-bootstrap "$@"
    ;;
  all)
    cargo run -p mg-dist -- build-all "$@"
    ;;
  megagate|megagate-web|megagate-ai|megagate-game|megagate-clo|megagate-cicd|megagate-iot|megagate-app|megagate-lib)
    cargo run -p mg-dist -- build "$package" "$@"
    ;;
  *)
    echo "unsupported package: $package"
    exit 1
    ;;
esac
