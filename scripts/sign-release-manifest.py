#!/usr/bin/env python3
"""Sign release-assets/manifest.json with the Ed25519 release key.

Reads the 64-hex-char seed from MGC_RELEASE_SIGNING_KEY. Signing uses
the VENDORED pure-python Ed25519 in scripts/_ed25519.py — no pip, no
network at signing time (P0-5: the old `pip install cryptography` made
every release depend on PyPI availability). Correctness of the vendored
code is pinned by scripts/test-release-manifest.sh (self-verify +
cross-check against the reference library when importable).

Without the secret:
- default: warn loudly, exit 0 (internal preview / transitional).
- MGC_RELEASE_REQUIRE_SIGNED=1: hard fail, exit 2 (stable releases
  must never ship unsigned — the publish job sets this).

Usage: python3 scripts/sign-release-manifest.py <assets-dir>
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))


def main(argv):
    if len(argv) != 2:
        print("usage: sign-release-manifest.py <assets-dir>", file=sys.stderr)
        return 2
    assets_dir = argv[1]
    seed_hex = os.environ.get("MGC_RELEASE_SIGNING_KEY", "").strip()
    if not seed_hex:
        if os.environ.get("MGC_RELEASE_REQUIRE_SIGNED", "").strip() in ("1", "true", "yes"):
            print(
                "::error::MGC_RELEASE_REQUIRE_SIGNED=1 but MGC_RELEASE_SIGNING_KEY "
                "is not set — refusing to publish an unsigned stable release",
                file=sys.stderr,
            )
            return 2
        print(
            "::warning::MGC_RELEASE_SIGNING_KEY not set — publishing "
            "UNSIGNED manifest (clients verify sha256 only)"
        )
        return 0
    try:
        seed = bytes.fromhex(seed_hex)
    except ValueError:
        print("::error::MGC_RELEASE_SIGNING_KEY is not valid hex", file=sys.stderr)
        return 2
    if len(seed) != 32:
        print("::error::signing key must be 64 hex chars (32-byte seed)", file=sys.stderr)
        return 2
    import _ed25519

    with open(os.path.join(assets_dir, "manifest.json"), "rb") as fh:
        manifest_bytes = fh.read()
    sig = _ed25519.sign(seed, manifest_bytes)
    # Self-verify before publishing the signature (a broken signer must
    # never ship a .sig file clients will reject).
    # (Tự verify trước khi publish chữ ký.)
    if not _ed25519.verify(_ed25519.public_key(seed), manifest_bytes, sig):
        print("::error::self-verification of the fresh signature FAILED", file=sys.stderr)
        return 2
    with open(os.path.join(assets_dir, "manifest.json.sig"), "w") as fh:
        fh.write(sig.hex() + "\n")
    print("manifest.json.sig written (vendored Ed25519, self-verified)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
