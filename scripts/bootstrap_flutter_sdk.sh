#!/usr/bin/env bash
# Resolve Flutter's SDK-owned flutter_tools dependencies before MGC guards app commands.
# Giải quyết dependency flutter_tools thuộc SDK Flutter trước khi process guard của MGC chạy.
set -euo pipefail

normalize_flutter_sdk_root() {
  local sdk_root="$1"
  local runner_os="$2"

  if [[ "$runner_os" == "Windows" ]]; then
    if ! command -v cygpath >/dev/null 2>&1; then
      echo "cygpath is required to resolve FLUTTER_ROOT on Windows" >&2
      return 1
    fi
    sdk_root="$(cygpath -u "$sdk_root")"
  fi
  printf '%s\n' "$sdk_root"
}

resolve_flutter_sdk_dart_bin() {
  local sdk_root="$1"
  local runner_os="$2"
  local executable="dart"
  if [[ "$runner_os" == "Windows" ]]; then
    executable="dart.exe"
  fi
  printf '%s\n' "$sdk_root/bin/cache/dart-sdk/bin/$executable"
}

bootstrap_flutter_sdk() {
  if [[ -z "${FLUTTER_ROOT:-}" ]]; then
    echo "FLUTTER_ROOT is required after setup-flutter-action" >&2
    return 1
  fi

  local runner_os="${RUNNER_OS:-}"
  local sdk_root
  sdk_root="$(normalize_flutter_sdk_root "$FLUTTER_ROOT" "$runner_os")"
  local dart_bin
  dart_bin="$(resolve_flutter_sdk_dart_bin "$sdk_root" "$runner_os")"
  if [[ ! -x "$dart_bin" ]]; then
    echo "Flutter SDK Dart executable is missing: $dart_bin" >&2
    return 1
  fi

  local flutter_bin="$sdk_root/bin/flutter"
  if [[ ! -x "$flutter_bin" ]]; then
    echo "Flutter CLI script is missing or not executable: $flutter_bin" >&2
    return 1
  fi

  # `--directory` keeps pub resolution inside the installed Flutter SDK, never the app.
  # `--directory` giữ Pub trong SDK Flutter đã cài, không đụng dependency của ứng dụng.
  "$dart_bin" pub --suppress-analytics --directory "$sdk_root/packages/flutter_tools" get --example

  # Flutter can lazily build its CLI and spawn Pub on the first invocation.
  # Flutter có thể dựng CLI trễ và spawn Pub ở lần gọi đầu tiên.
  "$flutter_bin" --version
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  bootstrap_flutter_sdk "$@"
fi
