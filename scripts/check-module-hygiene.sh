#!/usr/bin/env bash
# Module hygiene check (sys-mgc/21-repo-ops §1, 99-report 7.1)
# L0 (mgc-platform) + L1 (core/crates) cấm import L2/L3 (adapters/, cli/)
# Chạy: bash scripts/check-module-hygiene.sh
set -u

fail=0

check() {
    local dir="$1" label="$2" pattern="$3" skip="$4"
    while IFS=: read -r f line content; do
        echo "FAIL [$label] $f:$line: $content"
        fail=1
    done < <(grep -rnE "$pattern" "$dir" --include='*.rs' 2>/dev/null | grep -vE "$skip")
}

# Bỏ dòng thuộc khối #[cfg(test)] — grep từng file, skip dòng trong block
check_hard() {
    local dir="$1" label="$2" pattern="$3" skip="$4"
    while IFS=: read -r f line content; do
        echo "FAIL [$label] $f:$line: $content"
        fail=1
    done < <(find "$dir" -name '*.rs' -print0 2>/dev/null | while IFS= read -r -d '' file; do
        awk -v pat="$pattern" '
            /^\s*#\[cfg\(test\)\]/ { inseg=1; depth=0; next }
            inseg {
                for (i=1; i<=length($0); i++) {
                    c=substr($0,i,1)
                    if (c=="{") depth++
                    if (c=="}") depth--
                }
                if (depth<=0) inseg=0
                next
            }
            $0 ~ pat { print FILENAME ":" FNR ":" $0 }
        ' "$file"
    done | grep -vE "$skip")
}

# L0: mgc-platform cũng là core/crates — import chéo cấm (14 §1)
check "core/crates" "L1-import" 'use (adapters|cli)::' '^$'
# reqwest nơi chuẩn: mgc-http + mgc-fetcher + mgc-registry-server (upstream proxy); hardcode path cấm (14 §8)
# ALLOW: mgc-search clients for registry search (read-only, controlled)
check_hard "core/crates" "L1-hardcode" 'reqwest::|\$HOME|/Users/|/tmp/' 'crates/(mgc-http|mgc-fetcher|mgc-registry-server|mgc-search)/'

if [ "$fail" = "0" ]; then
    echo "OK: module hygiene pass"
fi
exit "$fail"
