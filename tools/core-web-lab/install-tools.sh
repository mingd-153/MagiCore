#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"

cat <<'EOF'
Core-Web external tool install lane

This script is intentionally conservative:
- it does not auto-download anything yet
- it reports missing tools and expected install sources
- it exists so the lab can move to a reproducible install flow next

Targets:
- RepoGraph
- OpenGrok
- Zoekt
- Hyperfine
- Syft
- Gitleaks
- Trivy
- pip-audit
- sonar-scanner
EOF

for tool in repograph opengrok zoekt-index hyperfine syft gitleaks trivy pip-audit sonar-scanner; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "[installed] $tool -> $(command -v "$tool")"
  else
    echo "[missing]   $tool"
  fi
done

echo
echo "Manifest: $LAB_DIR/external-tools.toml"
