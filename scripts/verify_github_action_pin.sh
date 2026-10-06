# Verify a GitHub Action tag against its immutable commit pin.
# Xác minh tag GitHub Action khớp commit SHA bất biến đã ghim.

verify_pin() {
    local repo="$1" tag="$2" expected="$3"
    local actual

    if ! command -v git >/dev/null 2>&1; then
        if [[ "${CI:-}" == "true" || "${CI:-}" == "1" || "${GITHUB_ACTIONS:-}" == "true" ]]; then
            echo "FAIL: CI cannot verify upstream action pins without git" >&2
            return 1
        fi
        echo "UNVERIFIED: git is unavailable; upstream action SHA was not checked" >&2
        return 2
    fi

    # Annotated tags point at a tag object; query the peeled commit first.
    # Tag có chú thích trỏ tới object riêng; ưu tiên truy vấn commit đã peel.
    actual="$(git ls-remote "https://github.com/${repo}.git" "refs/tags/${tag}^{}" 2>/dev/null | awk '{print $1}' || true)"
    if [ -z "$actual" ]; then
        actual="$(git ls-remote "https://github.com/${repo}.git" "refs/tags/${tag}" 2>/dev/null | awk '{print $1}' || true)"
    fi
    if [ -z "$actual" ]; then
        if [[ "${CI:-}" == "true" || "${CI:-}" == "1" || "${GITHUB_ACTIONS:-}" == "true" ]]; then
            echo "FAIL: CI cannot reach github.com to verify ${repo}@${tag}; refusing to pass the provenance contract" >&2
            return 1
        fi
        echo "UNVERIFIED: cannot reach github.com to verify ${repo}@${tag}; action pin was not checked" >&2
        return 2
    fi
    if [ "$actual" != "$expected" ]; then
        echo "FAIL: ${repo}@${tag} pin ${expected} does not match upstream commit ${actual}" >&2
        return 1
    fi
}
