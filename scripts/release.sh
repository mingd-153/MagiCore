#!/usr/bin/env bash
set -euo pipefail

profile="release"
target_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target_args+=(--target "$2")
      shift 2
      ;;
    --profile)
      profile="$2"
      shift 2
      ;;
    *)
      echo "unknown arg: $1"
      exit 1
      ;;
  esac
done

echo "==> building all packaged distributions"
cargo run -p mg-dist -- build-all --profile "$profile" "${target_args[@]}"
