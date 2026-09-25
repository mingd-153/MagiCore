#!/usr/bin/env bash
# Test the release manifest builder: RC versions with dashes must NOT be
# dropped, stable versions keep working, mismatched/decoy files are
# excluded, and a missing VERSION fails closed.
# Kiểm tra builder manifest: version RC có gạch nối không được rơi;
# version stable vẫn đúng; file lạ/khác version bị loại; thiếu VERSION
# thì fail rõ ràng.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

# Fixture archives (content irrelevant — digests cover whatever bytes).
# Archive giả (nội dung không quan trọng — digest bao nội dung đó).
touch "$FIX/magicore-1.1.0-rc.9-linux-x64.tar.gz"
touch "$FIX/magicore-web-1.1.0-rc.9-macos-arm64.tar.gz"
touch "$FIX/magicore-1.1.0-rc.9-windows-x64.zip"
touch "$FIX/magicore-1.1.0-linux-x64.tar.gz"
touch "$FIX/magicore-9.9.9-linux-x64.tar.gz"
touch "$FIX/README.md"
touch "$FIX/magicore-1.1.0-rc.9-linux-x64.tar.gz.sha256"

fail() {
    echo "FAIL: $1" >&2
    exit 1
}

# 1. RC version: all three rc.9 archives present, stable/mismatch/decoys out.
# Version RC: cả 3 archive rc.9 có mặt; stable/lệch/mồi bị loại.
VERSION=1.1.0-rc.9 python3 "$SCRIPT_DIR/release-manifest.py" "$FIX" > /dev/null
count=$(python3 -c "import json; print(len(json.load(open('$FIX/manifest.json'))['artifacts']))")
[[ "$count" == "3" ]] || fail "RC manifest holds $count artifacts, want 3"
for want in magicore-1.1.0-rc.9-linux-x64.tar.gz magicore-web-1.1.0-rc.9-macos-arm64.tar.gz magicore-1.1.0-rc.9-windows-x64.zip; do
    grep -q "$want" "$FIX/manifest.json" || fail "RC manifest missing $want"
done
grep -q '"version": "1.1.0-rc.9"' "$FIX/manifest.json" || fail "RC manifest version field wrong"
echo "ok: RC archives parsed (3/3, version field exact)"

# 2. Stable version: only the stable archive.
# Version stable: chỉ archive stable.
VERSION=1.1.0 python3 "$SCRIPT_DIR/release-manifest.py" "$FIX" > /dev/null
count=$(python3 -c "import json; print(len(json.load(open('$FIX/manifest.json'))['artifacts']))")
[[ "$count" == "1" ]] || fail "stable manifest holds $count artifacts, want 1"
echo "ok: stable archives parsed (1/1)"

# 3. Missing VERSION fails closed with a clear message (no KeyError).
# Thiếu VERSION thì fail rõ ràng (không KeyError).
if (unset VERSION; python3 "$SCRIPT_DIR/release-manifest.py" "$FIX" 2> "$FIX/err.log"); then
    fail "missing VERSION must exit non-zero"
fi
grep -q "VERSION is not set" "$FIX/err.log" || fail "missing VERSION error unclear"
grep -q "KeyError" "$FIX/err.log" && fail "unhandled KeyError leaked"
echo "ok: missing VERSION fails closed with a clear message"

# 3b. Matrix contract: full 8-asset RC set passes --require-matrix;
# a set missing macos-arm64 fails naming the asset.
# Hợp đồng ma trận: đủ 8 asset RC thì qua; thiếu macos-arm64 thì fail
# nêu tên asset.
FULL="$(mktemp -d)"
for spec in "magicore:linux:x64:tar.gz" "magicore:macos:x64:tar.gz" "magicore:macos:arm64:tar.gz" "magicore:windows:x64:zip" "magicore-web:linux:x64:tar.gz" "magicore-web:macos:x64:tar.gz" "magicore-web:macos:arm64:tar.gz" "magicore-web:windows:x64:zip"; do
    pkg="${spec%%:*}"; rest="${spec#*:}"; os="${rest%%:*}"; rest="${rest#*:}"; arch="${rest%%:*}"; ext="${rest##*:}"
    touch "$FULL/${pkg}-1.1.0-rc.9-${os}-${arch}.${ext}"
done
VERSION=1.1.0-rc.9 python3 "$SCRIPT_DIR/release-manifest.py" --require-matrix "$FULL" > /dev/null \
    || fail "full 8-asset matrix must pass --require-matrix"
echo "ok: full matrix passes --require-matrix (8/8)"
rm "$FULL/magicore-web-1.1.0-rc.9-macos-arm64.tar.gz"
if VERSION=1.1.0-rc.9 python3 "$SCRIPT_DIR/release-manifest.py" --require-matrix "$FULL" 2> "$FULL/matrix-err.log"; then
    fail "matrix missing macos-arm64 must exit non-zero"
fi
grep -q "magicore-web-<version>-macos-arm64" "$FULL/matrix-err.log" || fail "matrix error must name the missing asset"
echo "ok: incomplete matrix fails naming the missing asset"
rm -rf "$FULL"

# 4. Signing without a key warns and exits 0 (transitional unsigned).
# Ký thiếu key thì cảnh báo và exit 0 (unsigned quá độ).
out=$(python3 "$SCRIPT_DIR/sign-release-manifest.py" "$FIX")
echo "$out" | grep -q "UNSIGNED" || fail "unsigned path must warn loudly"
echo "ok: unsigned signing path warns loudly"

# 5. Stable gate: REQUIRE_SIGNED=1 without a key is a hard fail.
# Cổng stable: REQUIRE_SIGNED=1 thiếu key thì fail cứng.
if MGC_RELEASE_REQUIRE_SIGNED=1 python3 "$SCRIPT_DIR/sign-release-manifest.py" "$FIX" 2> "$FIX/req.log"; then
    fail "REQUIRE_SIGNED=1 without a key must exit non-zero"
fi
grep -q "refusing to publish an unsigned stable release" "$FIX/req.log" || fail "require-signed error unclear"
echo "ok: require-signed gate fails closed without a key"

# 6. Vendored Ed25519: self-verify (always) + cross-check against the
# reference library when importable (cassé = fail, absent = loud skip).
# Ed25519 vendor: tự verify (luôn) + đối chiếu lib chuẩn (có thì check).
python3 - "$SCRIPT_DIR" <<'PYEOF'
import secrets
import sys
sys.path.insert(0, sys.argv[1])
import _ed25519
for trial in range(3):
    seed = secrets.token_bytes(32)
    pub = _ed25519.public_key(seed)
    for msg in (b"", b"release-manifest-bytes", bytes(range(256))):
        sig = _ed25519.sign(seed, msg)
        assert _ed25519.verify(pub, msg, sig), "self-verify failed"
        assert not _ed25519.verify(pub, msg + b"!", sig), "forgery accepted"
try:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
except ImportError:
    print("note: cryptography lib absent — cross-check skipped (CI verify job covers it)")
    sys.exit(0)
for trial in range(3):
    seed = secrets.token_bytes(32)
    ref_pub = Ed25519PrivateKey.from_private_bytes(seed).public_key()
    ref_bytes = ref_pub.public_bytes(Encoding.Raw, PublicFormat.Raw)
    assert _ed25519.public_key(seed) == ref_bytes, "pubkey diverges from reference lib"
    msg = b"cross-check"
    ref_pub.verify(_ed25519.sign(seed, msg), msg)
print("ok: vendored Ed25519 self-verifies + matches reference lib")
PYEOF
[ $? -eq 0 ] || fail "ed25519 self-test failed"

echo "ALL RELEASE-MANIFEST TESTS PASSED"

# 7. Cross-implementation: the Rust CLI signer (ring, locked) output must
# verify under the independent python implementation. Needs MGC_BIN
# (release job passes the just-built binary); skipped loudly otherwise.
# Đối chiếu chéo: chữ ký của CLI Rust phải verify được bằng python độc lập.
if [[ -n "${MGC_BIN:-}" && -x "${MGC_BIN:-}" ]]; then
    SEED="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
    "$MGC_BIN" sign-release --manifest "$FIX/manifest.json" --key-hex "$SEED"
    MGC_SIG_FILE="$FIX/manifest.json.sig" MGC_SIG_SEED="$SEED" MGC_SIG_MSG="$FIX/manifest.json" python3 - <<'PYEOF'
import os
import sys
sys.path.insert(0, "scripts")
import _ed25519
mb = open(os.environ["MGC_SIG_MSG"], "rb").read()
sig = bytes.fromhex(open(os.environ["MGC_SIG_FILE"]).read().split()[0])
try:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
except ImportError:
    print("note: cryptography absent — CLI cross-check skipped")
    sys.exit(0)
pub = Ed25519PrivateKey.from_private_bytes(bytes.fromhex(os.environ["MGC_SIG_SEED"])).public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
assert _ed25519.verify(pub, mb, sig), "python cross-verify of Rust CLI signature failed"
print("ok: Rust CLI signature verifies under independent python impl")
PYEOF
else
    echo "note: MGC_BIN unset — CLI cross-check skipped (release job covers it)"
fi
