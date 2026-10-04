#!/usr/bin/env bash
# Resolve Flutter's SDK-owned flutter_tools dependencies before MGC guards app commands.
# Giải quyết dependency flutter_tools thuộc SDK Flutter trước khi process guard của MGC chạy.
set -euo pipefail

if [[ -z "${FLUTTER_ROOT:-}" ]]; then
  echo "FLUTTER_ROOT is required after setup-flutter-action" >&2
  exit 1
fi

sdk_root="$FLUTTER_ROOT"
if [[ "${RUNNER_OS:-}" == "Windows" ]]; then
  if ! command -v cygpath >/dev/null 2>&1; then
    echo "cygpath is required to resolve FLUTTER_ROOT on Windows" >&2
    exit 1
  fi
  sdk_root="$(cygpath -u "$sdk_root")"
  dart_bin="$sdk_root/bin/cache/dart-sdk/bin/dart.exe"
else
  dart_bin="$sdk_root/bin/cache/dart-sdk/bin/dart"
fi

if [[ ! -x "$dart_bin" ]]; then
  echo "Flutter SDK Dart executable is missing: $dart_bin" >&2
  exit 1
fi

# `--directory` keeps pub resolution inside the installed Flutter SDK, never the app.
# `--directory` giữ Pub trong SDK Flutter đã cài, không đụng dependency của ứng dụng.
"$dart_bin" pub --suppress-analytics --directory "$sdk_root/packages/flutter_tools" get --example
