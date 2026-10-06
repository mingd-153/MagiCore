"""Minimal Ed25519 signing (RFC 8032, SHA-512) — vendored so release
signing needs NO network/pip at signing time (P0-5: the old `pip install
cryptography` made every release depend on PyPI availability).

Scope: SIGN ONLY with a 32-byte seed (what the release job needs).
Verification lives in Rust (mgc self-update) and is covered by its own
roundtrip tests. Correctness is pinned by the RFC 8032 §A test vectors
in scripts/test-release-manifest.sh — any edit here must keep them
green.
"""

import hashlib

# Curve25519/Ed25519 domain parameters (RFC 8032 §5.1).
# WARNING: these two integers were once misremembered here and caught
# by the cross-check test — never hand-edit; re-derive as y = 4/5 mod p
# and x = xrecover(y), then verify -x²+y² == 1+d·x²·y² (see test).
P = 2**255 - 19
D = -121665 * pow(121666, -1, P) % P
Q = 2**252 + 27742317777372353535851937790883648493
GX = 15112221349535400772501151409588531511454012693041857206046113283949847762202
GY = 46316835694926478169428394003475163141307993866256225615783033603165251855960


def _xrecover(y):
    """Recover x from y (RFC 8032: x = ±sqrt((y²-1)/(d·y²+1)), even root)."""
    y2 = (y * y) % P
    num = (y2 - 1) % P
    den = (D * y2 + 1) % P
    x = pow(num * pow(den, -1, P) % P, (P + 3) // 8, P)
    if (x * x - num * pow(den, -1, P)) % P != 0:
        x = (x * pow(2, (P - 1) // 4, P)) % P
    if x & 1:
        x = P - x
    return x


def _encodepoint(x, y):
    """Compressed Edwards y + low bit of x (little-endian 32 bytes)."""
    bits = [(y >> i) & 1 for i in range(256)] + [0] * 8
    bits[255] = x & 1
    out = bytearray(32)
    for i in range(32):
        byte = 0
        for j in range(8):
            byte |= bits[8 * i + j] << j
        out[i] = byte
    return bytes(out)


def _scalarmult_base(scalar):
    """Naive double-and-add on the base point (correct, not fast —
    one 32-byte signature per release is all we ever need)."""
    scalar %= Q
    rx, ry = 0, 1  # identity in extended coords (x=0, y=1)
    # Extended coordinates: (X, Y, Z, T) with x=X/Z, y=Y/Z.
    rx, ry, rz, rt = 0, 1, 1, 0
    qx, qy = GX, GY
    # Precompute odd multiples via repeated doubling on extended coords.
    # Simple approach: binary method with complete addition formulas
    # (Hisil et al. 2008, extended coordinates).
    px, py, pz, pt = qx, qy, 1, (qx * qy) % P

    def add(ax, ay, az, at, bx, by, bz, bt):
        a = ((ay - ax) * (by - bx)) % P
        b = ((ay + ax) * (by + bx)) % P
        c = (at * 2 * D * bt) % P
        d = (az * 2 * bz) % P
        e = (b - a) % P
        f = (d - c) % P
        g = (d + c) % P
        h = (b + a) % P
        return (
            (e * f) % P,
            (g * h) % P,
            (f * g) % P,
            (e * h) % P,
        )

    # Binary double-and-add from the top bit.
    rx, ry, rz, rt = 0, 1, 1, 0
    for i in range(252, -1, -1):
        rx, ry, rz, rt = add(rx, ry, rz, rt, rx, ry, rz, rt)
        if (scalar >> i) & 1:
            rx, ry, rz, rt = add(rx, ry, rz, rt, px, py, pz, pt)
    # Convert back: x = X/Z, y = Y/Z.
    inv = pow(rz, -1, P)
    return (rx * inv) % P, (ry * inv) % P


def _clamp(raw32: bytes) -> int:
    """RFC 8032 §5.1.5 scalar clamping: clear the lowest THREE bits,
    clear bit 255, set bit 254. (An earlier revision cleared only bit
    0 — caught by the cross-check: zero-seeds with low bits already
    clear passed while random seeds failed.)
    (Kẹp scalar đúng RFC: xóa 3 bit thấp, xóa bit 255, đặt bit 254.)"""
    return (int.from_bytes(raw32, "little") & ~(0b111 | (1 << 255))) | (1 << 254)


def _hint(raw):
    """SHA-512 interpreted as a little-endian scalar mod Q."""
    return int.from_bytes(hashlib.sha512(raw).digest(), "little") % Q


def sign(seed: bytes, message: bytes) -> bytes:
    """Detached Ed25519 signature (64 bytes) for seed + message."""
    if len(seed) != 32:
        raise ValueError("seed must be 32 bytes")
    h = hashlib.sha512(seed).digest()
    a = _clamp(h[:32])
    prefix = h[32:]
    pub_x, pub_y = _scalarmult_base(a)
    pub = _encodepoint(pub_x, pub_y)
    r = _hint(prefix + message)
    rx, ry = _scalarmult_base(r)
    r_enc = _encodepoint(rx, ry)
    s = (r + _hint(r_enc + pub + message) * a) % Q
    return r_enc + s.to_bytes(32, "little")


def public_key(seed: bytes) -> bytes:
    """Compressed Ed25519 public key (32 bytes) for a seed."""
    if len(seed) != 32:
        raise ValueError("seed must be 32 bytes")
    h = hashlib.sha512(seed).digest()
    a = _clamp(h[:32])
    x, y = _scalarmult_base(a)
    return _encodepoint(x, y)


def _decodepoint(enc: bytes):
    """Compressed bytes → (x, y) or None when out of range."""
    if len(enc) != 32:
        return None
    y = int.from_bytes(enc, "little") & ~(1 << 255)
    if y >= P:
        return None
    x = _xrecover(y)
    if (x & 1) != (enc[31] >> 7):
        x = P - x
    return x, y


def verify(public_key: bytes, message: bytes, signature: bytes) -> bool:
    """Check a detached signature (pure python — lets the release test
    self-verify without any third-party library).
    (Kiểm chữ ký thuần python — test tự verify không cần lib ngoài.)"""
    if len(signature) != 64:
        return False
    a_pt = _decodepoint(public_key)
    r_pt = _decodepoint(signature[:32])
    if a_pt is None or r_pt is None:
        return False
    s = int.from_bytes(signature[32:], "little")
    if s >= Q:
        return False
    h = _hint(signature[:32] + public_key + message)
    # R == [s]B − [h]A via two mults + one affine addition.
    sb_x, sb_y = _scalarmult_base(s)
    ha_x, ha_y = _mul_point(h, a_pt)
    # Negate [h]A: (-x, y), then add.
    diff = _add_points((sb_x, sb_y), (P - ha_x, ha_y))
    return _encodepoint(*diff) == signature[:32]


def _mul_point(scalar, pt):
    """Scalar × arbitrary point (binary double-and-add, affine)."""
    rx, ry = 0, 1
    bx, by = pt
    while scalar:
        if scalar & 1:
            rx, ry = _add_points((rx, ry), (bx, by))
        bx, by = _add_points((bx, by), (bx, by))
        scalar >>= 1
    return rx, ry


def _add_points(p, q):
    """Twisted-Edwards affine addition (a=-1)."""
    (x1, y1), (x2, y2) = p, q
    x1x2, y1y2 = (x1 * x2) % P, (y1 * y2) % P
    den1 = (1 + D * x1x2 * y1y2) % P
    den2 = (1 - D * x1x2 * y1y2) % P
    return (
        ((x1 * y2 + x2 * y1) % P * pow(den1, -1, P)) % P,
        ((y1y2 + x1x2) % P * pow(den2, -1, P)) % P,
    )
