//! `sign-release-manifest` — sign a release manifest with the Ed25519
//! release key through the REVIEWED `ring` backend (mgc-crypto, locked
//! in Cargo.lock) — no pip, no network at signing time (P0: the old
//! python path `pip install`ed cryptography on every release).
//!
//! Contract (mirrors scripts/sign-release-manifest.py, which stays as
//! the independent cross-check oracle):
//! - key from `--key-hex` or `MGC_RELEASE_SIGNING_KEY` (64 hex chars).
//! - no key: loud warning + Ok (internal preview / transitional).
//! - no key + `MGC_RELEASE_REQUIRE_SIGNED=1`: hard Err (stable releases
//!   must never ship unsigned — the publish job sets this).
//! - the fresh signature is self-verified BEFORE writing `<manifest>.sig`.
//!
//! (Ký manifest release bằng backend `ring` đã review — không pip,
//! không mạng lúc ký.)

use anyhow::{Context, Result};

/// Sign `manifest_path` (default `manifest.json`), writing hex + newline
/// to `<manifest>.sig`. Pure file I/O + ring — hermetic and unit-tested.
/// (Ký file manifest, ghi chữ ký hex.)
pub fn run(manifest: Option<String>, key_hex: Option<String>) -> Result<()> {
    let seed_hex = key_hex
        .or_else(|| std::env::var("MGC_RELEASE_SIGNING_KEY").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let require_signed = is_require_signed();
    run_with_key(
        &manifest.unwrap_or_else(|| "manifest.json".to_string()),
        seed_hex.as_deref(),
        require_signed,
    )
}

/// Core signing logic with explicit inputs (no env reads — directly
/// unit-testable).
/// (Logic ký với input tường minh — test được trực tiếp.)
pub fn run_with_key(manifest: &str, seed_hex: Option<&str>, require_signed: bool) -> Result<()> {
    let manifest_path = std::path::PathBuf::from(manifest);
    let Some(seed_hex) = seed_hex.filter(|s| !s.is_empty()) else {
        if require_signed {
            anyhow::bail!(
                "MGC_RELEASE_REQUIRE_SIGNED=1 but no signing key (--key-hex or MGC_RELEASE_SIGNING_KEY) — refusing to publish an unsigned stable release"
            );
        }
        eprintln!(
            "warning: no signing key — publishing UNSIGNED manifest (clients verify sha256 only)"
        );
        return Ok(());
    };
    let seed_bytes = hex_to_seed(seed_hex)?;
    let signer = mgc_crypto::ed25519_signer::Ed25519Signer::from_seed(&seed_bytes)
        .map_err(|e| anyhow::anyhow!("invalid signing seed: {e:?}"))?;
    let manifest_bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let sig = signer.sign(&manifest_bytes);
    // Self-verify before publishing the signature (a broken signer must
    // never ship a .sig file clients will reject).
    // (Tự verify trước khi publish chữ ký.)
    mgc_crypto::ed25519_signer::verify_signature(&signer.public_key(), &manifest_bytes, &sig)
        .map_err(|e| anyhow::anyhow!("self-verification of the fresh signature FAILED: {e:?}"))?;
    let sig_path = manifest_path.with_extension("json.sig");
    std::fs::write(&sig_path, format!("{}\n", hex::encode(&sig.0)))
        .with_context(|| format!("write {}", sig_path.display()))?;
    println!("manifest signature written (ring Ed25519, self-verified)");
    Ok(())
}

fn is_require_signed() -> bool {
    matches!(
        std::env::var("MGC_RELEASE_REQUIRE_SIGNED")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

fn hex_to_seed(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str.trim())
        .map_err(|e| anyhow::anyhow!("signing key is not valid hex: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signing key must be 64 hex chars (32-byte seed)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mgc-sign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sign_is_deterministic_and_self_verified() {
        let dir = tmp();
        let manifest = dir.join("manifest.json");
        std::fs::write(&manifest, b"{\"version\":\"9.9.9\"}").unwrap();
        let key = "0".repeat(64);
        run(Some(manifest.display().to_string()), Some(key.to_string())).unwrap();
        let sig1 = std::fs::read_to_string(dir.join("manifest.json.sig")).unwrap();
        run(Some(manifest.display().to_string()), Some(key.to_string())).unwrap();
        let sig2 = std::fs::read_to_string(dir.join("manifest.json.sig")).unwrap();
        assert_eq!(sig1, sig2, "Ed25519 signing is deterministic");
        assert_eq!(sig1.len(), 128 + 1, "64-byte hex + newline");
    }

    #[test]
    fn bad_key_shapes_fail_closed() {
        let dir = tmp();
        let manifest = dir.join("manifest.json");
        std::fs::write(&manifest, b"{}").unwrap();
        assert!(run(Some(manifest.display().to_string()), Some("zz".to_string())).is_err());
        assert!(
            run(
                Some(manifest.display().to_string()),
                Some("abcd".to_string())
            )
            .is_err()
        );
        assert!(
            run(
                Some("/no/such/manifest.json".to_string()),
                Some("00".repeat(32))
            )
            .is_err()
        );
    }

    #[test]
    fn missing_key_without_gate_warns_ok() {
        // No env touched: explicit None + require=false exercises the
        // transitional path hermetically.
        // (Không chạm env: None tường minh + require=false.)
        let dir = tmp();
        let manifest = dir.join("manifest.json");
        std::fs::write(&manifest, b"{}").unwrap();
        let out = run_with_key(&manifest.display().to_string(), None, false);
        assert!(out.is_ok(), "transitional unsigned path warns but exits 0");
        assert!(
            !dir.join("manifest.json.sig").exists(),
            "no .sig file on the unsigned path"
        );
        assert!(
            run_with_key(&manifest.display().to_string(), None, true).is_err(),
            "require-signed without a key must fail"
        );
    }
}
