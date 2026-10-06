//! Core-identity attestation — cryptographic binding for `.mgc.core`.
//! Chứng thực identity core — ràng buộc mật mã cho `.mgc.core`.
//!
//! The marker file is one plain-text line (`project.rs` states it is NOT a
//! signature). This module binds `(core, mgc.toml digest)` with Ed25519 to
//! a key in the home keyring and stores the attestation OUTSIDE the project
//! (`~/.magicore/identities/<path-hash>/attestation.json`), so editing the
//! project cannot forge the anchor. Absence degrades to a warning
//! (backwards compatibility); a PRESENT but mismatching attestation fails
//! closed with remediation — never silently re-anchored.
//! (Marker là 1 dòng plain-text. Module này ràng buộc `(core, digest
//! mgc.toml)` bằng Ed25519 với key trong keyring home và lưu chứng thực
//! NGOÀI project, nên sửa project không giả được anchor. Vắng anchor thì
//! cảnh báo (tương thích cũ); anchor CÓ mà lệch thì fail kèm remediation.)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Domain-separated message prefix — Tiền tố tách miền ký.
pub const ATTESTATION_MESSAGE_DOMAIN: &str = "mgc-core-attestation/v1";
/// Attestation file name inside a project slot — Tên file chứng thực.
pub const ATTESTATION_FILE: &str = "attestation.json";
/// Env override for anchor enforcement — Env chỉnh enforcement.
pub const ANCHOR_MODE_ENV: &str = "MGC_TRUST_ANCHOR";

/// Enforcement mode — Chế độ bắt buộc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorMode {
    /// Skip attestation checks entirely.
    Off,
    /// Missing anchor warns; mismatching anchor warns (default).
    Warn,
    /// Missing anchor fails; mismatching anchor fails.
    Require,
}

impl AnchorMode {
    /// Parse `off` | `warn` | `require` (case-insensitive, `true` = require).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "false" | "0" | "no" => Some(Self::Off),
            "warn" | "" => Some(Self::Warn),
            "require" | "true" | "1" | "yes" | "strict" => Some(Self::Require),
            _ => None,
        }
    }

    /// Effective mode: explicit env wins, otherwise warn.
    /// (CI does NOT auto-escalate: existing projects without anchors must
    /// keep working until they attest; CI opts in with `require`.)
    pub fn effective() -> Self {
        std::env::var(ANCHOR_MODE_ENV)
            .ok()
            .and_then(|raw| Self::parse(&raw))
            .unwrap_or(Self::Warn)
    }
}

/// A previous signing key kept for verifying older attestations.
/// Key ký cũ giữ lại để verify chứng thực cũ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviousKey {
    /// Key id (BLAKE3-8 of the public key, keyring convention).
    pub key_id: String,
    /// Base64 Ed25519 public key.
    pub public_key: String,
}

/// Core-identity attestation — Chứng thực identity core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attestation {
    /// Canonical core name (`web`, `clo`, …).
    pub core: String,
    /// BLAKE3 hex of the exact `mgc.toml` bytes at attest time.
    pub config_digest: String,
    /// Absolute project path the attestation was made for.
    pub project_path: String,
    /// Signing key id.
    pub key_id: String,
    /// Base64 Ed25519 public key (verify without the keyring).
    pub public_key: String,
    /// Rotated-out keys, newest first (at most one generation kept).
    #[serde(default)]
    pub prev_keys: Vec<PreviousKey>,
    /// RFC 3339 attest time.
    pub attested_at: String,
    /// mgc version that attested.
    pub mgc_version: String,
    /// Base64 Ed25519 signature over the canonical message.
    pub signature: String,
}

/// Attestation check outcome — Kết quả kiểm tra chứng thực.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestationStatus {
    /// No attestation slot for this project.
    Absent,
    /// Signature and digest match the live project.
    Valid,
    /// An attestation exists but does not match (reason included).
    Mismatch(String),
}

/// Canonical signed message — Thông điệp ký canonical.
pub fn canonical_message(core: &str, config_digest: &str) -> String {
    format!("{ATTESTATION_MESSAGE_DOMAIN}\n{core}\n{config_digest}")
}

/// BLAKE3 hex of bytes — BLAKE3 hex của bytes.
pub fn blake3_hex(data: &[u8]) -> String {
    let hash = mgc_crypto::blake3_signer::Blake3Hasher::hash_bytes(data);
    hex::encode(hash.0)
}

/// Directory slot for a project under an identities root.
/// Slot thư mục của project dưới identities root.
pub fn attestation_dir(identities_dir: &Path, project_root: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf())
        .to_string_lossy()
        .to_string();
    identities_dir.join(blake3_hex(canonical.as_bytes()))
}

/// Read `mgc.toml` bytes without following symlinks — fail-closed on
/// unreadable input (the caller decides warn vs fail).
pub fn read_config_bytes(project_root: &Path) -> anyhow::Result<Vec<u8>> {
    let path = project_root.join("mgc.toml");
    let text = crate::project::read_regular_project_text(&path, "project config")?
        .ok_or_else(|| anyhow::anyhow!("project has no mgc.toml"))?;
    Ok(text.into_bytes())
}

/// Load (or create) a file keyring at an explicit path — mirrors
/// `Keyring::init_if_not_exists` without pinning the home default, so
/// tests can isolate keys in tempdirs.
/// (Nạp (hoặc tạo) keyring tại path tường minh — như
/// `init_if_not_exists` nhưng không ghim home default để test cô lập.)
pub fn load_or_create_keyring(keyring_path: &Path) -> anyhow::Result<mgc_crypto::keyring::Keyring> {
    if keyring_path.exists() {
        return mgc_crypto::keyring::Keyring::load(keyring_path)
            .map_err(|error| anyhow::anyhow!("cannot load keyring: {error}"));
    }
    // Do NOT create parent dirs here: Keyring::save validates the path
    // before creating parents, and a pre-created parent changes the
    // canonicalization outcome (symlinked TMPDIR roots). Let save own it.
    // (Không tạo dir cha trước: save validate path trước khi tạo dir.)
    let mut keyring = mgc_crypto::keyring::Keyring::new();
    let key_pair = mgc_crypto::keyring::KeyPair::generate()
        .map_err(|error| anyhow::anyhow!("cannot generate key: {error}"))?;
    keyring.add_key(key_pair);
    keyring
        .save(keyring_path)
        .map_err(|error| anyhow::anyhow!("cannot save keyring: {error}"))?;
    Ok(keyring)
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Attest version tag — the mgc build that attested (compile-time).
/// (Version mgc đã chứng thực — compile-time.)
pub fn attester_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Attest a project core: sign `(core, mgc.toml digest)` and store the
/// attestation outside the project. Idempotent when the live project
/// already matches the stored attestation (unless `force`).
/// (Chứng thực core project: ký và lưu ngoài project. Idempotent khi
/// project hiện tại đã khớp chứng thực cũ (trừ khi `force`).)
pub fn attest(
    project_root: &Path,
    core: &str,
    keyring_path: &Path,
    identities_dir: &Path,
    key_id: Option<&str>,
    force: bool,
    rotate: bool,
) -> anyhow::Result<Attestation> {
    let core = crate::project::ProjectConfig::canonical_core(core);
    if !crate::project::ProjectConfig::is_known_core(&core) {
        anyhow::bail!("cannot attest unknown core '{core}'");
    }
    let live_core = crate::project::ProjectConfig::read_core_marker_identity(project_root)?
        .ok_or_else(|| anyhow::anyhow!("project has no core marker to attest"))?;
    if live_core != core {
        anyhow::bail!(
            "refusing to attest core '{core}' because the live project marker is '{live_core}'"
        );
    }
    let slot = attestation_dir(identities_dir, project_root);
    let path = slot.join(ATTESTATION_FILE);
    if !force && !rotate && path.is_file() {
        match verify(project_root, identities_dir)? {
            AttestationStatus::Valid => {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| anyhow::anyhow!("cannot read attestation: {error}"))?;
                let existing: Attestation = serde_json::from_str(&text)
                    .map_err(|error| anyhow::anyhow!("cannot parse attestation: {error}"))?;
                return Ok(existing);
            }
            AttestationStatus::Mismatch(reason) => {
                anyhow::bail!(
                    "existing core attestation does not match ({reason}); pass --re-attest after intentional changes"
                );
            }
            AttestationStatus::Absent => {}
        }
    }
    let config = read_config_bytes(project_root)?;
    let digest = blake3_hex(&config);
    let mut keyring = load_or_create_keyring(keyring_path)?;
    let mut key_pair = if let Some(id) = key_id {
        keyring
            .get_key(id)
            .ok_or_else(|| anyhow::anyhow!("key not found in keyring: {id}"))?
            .clone()
    } else {
        keyring
            .default_key()
            .ok_or_else(|| anyhow::anyhow!("keyring has no default key — run `mgc trust init`"))?
            .clone()
    };
    let mut prev_keys = Vec::new();
    if rotate {
        // Determine the post-rotation signing key FIRST, then record the
        // previous key exactly when it differs — comparing against a key
        // that is about to be replaced would silently drop history.
        // (Xác định key ký sau-rotation TRƯỚC, rồi mới ghi key cũ khi
        // khác — so với key sắp bị thay sẽ mất lịch sử.)
        // A fresh key for rotation: generate only when the caller did not
        // name a key (named-key rotation switches to that key instead).
        // (Key mới cho rotation: chỉ sinh khi caller không chỉ định key.)
        if key_id.is_none() {
            let fresh = mgc_crypto::keyring::KeyPair::generate()
                .map_err(|error| anyhow::anyhow!("cannot generate key: {error}"))?;
            let fresh_id = fresh.key_id.clone();
            keyring.add_key(fresh);
            keyring
                .set_default(&fresh_id)
                .map_err(|error| anyhow::anyhow!("cannot set default key: {error}"))?;
            keyring
                .save(keyring_path)
                .map_err(|error| anyhow::anyhow!("cannot save keyring: {error}"))?;
            key_pair = keyring
                .get_key(&fresh_id)
                .ok_or_else(|| anyhow::anyhow!("fresh key vanished from keyring"))?
                .clone();
        }
        if path.is_file() {
            // Keep exactly one generation back — rotation chains must
            // not accumulate unbounded trust history.
            // (Chỉ giữ 1 đời key cũ — chuỗi rotation không phình.)
            prev_keys.extend(previous_key_for(&path, &key_pair.key_id));
        }
    }
    sign_and_store(
        project_root,
        &core,
        &digest,
        &key_pair,
        prev_keys,
        &slot,
        &path,
    )
}

/// Previous signing key for rotation continuity, if the stored
/// attestation names a different key — `None` on first attest,
/// unparseable slots, or same-key rotation.
/// (Key ký cũ để rotation liên tục — `None` khi attest đầu, slot hỏng,
/// hoặc cùng key.)
fn previous_key_for(path: &Path, current_key_id: &str) -> Option<PreviousKey> {
    let text = std::fs::read_to_string(path).ok()?;
    let old: Attestation = serde_json::from_str(&text).ok()?;
    (old.key_id != current_key_id).then_some(PreviousKey {
        key_id: old.key_id,
        public_key: old.public_key,
    })
}

#[allow(clippy::too_many_arguments)]
fn sign_and_store(
    project_root: &Path,
    core: &str,
    digest: &str,
    key_pair: &mgc_crypto::keyring::KeyPair,
    prev_keys: Vec<PreviousKey>,
    slot: &Path,
    path: &Path,
) -> anyhow::Result<Attestation> {
    let signer = key_pair
        .signer()
        .map_err(|error| anyhow::anyhow!("cannot use key: {error}"))?;
    let signature = signer.sign(canonical_message(core, digest).as_bytes());
    let attestation = Attestation {
        core: core.to_string(),
        config_digest: digest.to_string(),
        project_path: project_root.to_string_lossy().to_string(),
        key_id: key_pair.key_id.clone(),
        public_key: key_pair.public_key.to_base64(),
        prev_keys,
        attested_at: now_rfc3339(),
        mgc_version: attester_version().to_string(),
        signature: signature.to_base64(),
    };
    std::fs::create_dir_all(slot)
        .map_err(|error| anyhow::anyhow!("cannot create attestation slot: {error}"))?;
    let text = serde_json::to_string_pretty(&attestation)
        .map_err(|error| anyhow::anyhow!("cannot serialize attestation: {error}"))?;
    std::fs::write(path, text)
        .map_err(|error| anyhow::anyhow!("cannot write attestation: {error}"))?;
    Ok(attestation)
}

/// Verify the live project against its attestation. Verification needs
/// no secrets: the attestation embeds every public key it trusts, so a
/// machine holding no private key can still check integrity.
/// (Verify project hiện tại với chứng thực của nó. Verify không cần
/// secret: chứng thực nhúng mọi public key nó tin.)
pub fn verify(project_root: &Path, identities_dir: &Path) -> anyhow::Result<AttestationStatus> {
    let path = attestation_dir(identities_dir, project_root).join(ATTESTATION_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AttestationStatus::Absent);
        }
        Err(error) => return Err(anyhow::anyhow!("cannot read attestation: {error}")),
    };
    let attestation: Attestation = serde_json::from_str(&text)
        .map_err(|error| anyhow::anyhow!("attestation is not valid JSON: {error}"))?;
    let live_core = match crate::project::ProjectConfig::read_core_marker_identity(project_root) {
        Ok(Some(core)) => core,
        Ok(None) => {
            return Ok(AttestationStatus::Mismatch(
                "project core marker is missing".to_string(),
            ));
        }
        Err(error) => {
            return Ok(AttestationStatus::Mismatch(format!(
                "project core marker is invalid: {error}"
            )));
        }
    };
    if live_core != attestation.core {
        return Ok(AttestationStatus::Mismatch(format!(
            "live core marker is '{live_core}', but the attestation binds '{}'",
            attestation.core
        )));
    }
    let config = match read_config_bytes(project_root) {
        Ok(config) => config,
        Err(_) => {
            return Ok(AttestationStatus::Mismatch(
                "project mgc.toml is missing or unreadable".to_string(),
            ));
        }
    };
    let digest = blake3_hex(&config);
    if digest != attestation.config_digest {
        return Ok(AttestationStatus::Mismatch(
            "mgc.toml changed since attestation (re-attest after intentional edits)".to_string(),
        ));
    }
    let message = canonical_message(&attestation.core, &attestation.config_digest);
    let mut candidates = vec![attestation.public_key.clone()];
    for previous in &attestation.prev_keys {
        candidates.push(previous.public_key.clone());
    }
    for public_key_b64 in candidates {
        let Ok(public_key) =
            mgc_crypto::ed25519_signer::Ed25519PublicKey::from_base64(&public_key_b64)
        else {
            continue;
        };
        let Ok(signature) =
            mgc_crypto::ed25519_signer::Ed25519Signature::from_base64(&attestation.signature)
        else {
            continue;
        };
        if mgc_crypto::ed25519_signer::verify_signature(&public_key, message.as_bytes(), &signature)
            .is_ok()
        {
            return Ok(AttestationStatus::Valid);
        }
    }
    Ok(AttestationStatus::Mismatch(
        "signature does not verify against the attested keys".to_string(),
    ))
}

/// Enforce the live attestation for an already-validated core marker.
/// Called at the end of `read_core_marker` so every core dispatch is
/// covered. Absent anchor warns (backwards compatibility, unless
/// `require`); mismatching anchor fails in `require` mode and warns
/// otherwise — never silently re-anchored. Unreadable state warns and
/// proceeds in Warn mode; Require fails closed when home or state is
/// unavailable.
/// (Bắt buộc chứng thực cho marker đã validate. Vắng anchor thì cảnh
/// báo; lệch anchor thì fail ở mode require, cảnh báo ở mode warn —
/// không bao giờ tự ghi đè. Home/state không đọc được fail-closed ở
/// Require, còn Warn chỉ cảnh báo để giữ tương thích.)
pub fn enforce_live_attestation(project_root: &Path, core: &str) -> anyhow::Result<()> {
    let mode = AnchorMode::effective();
    if mode == AnchorMode::Off {
        return Ok(());
    }
    let identities = match mgc_platform::paths::GlobalPaths::new() {
        Ok(paths) => paths.identities,
        Err(error) => {
            let message = format!(
                "cannot access global identities dir ({error}) — core attestation cannot be checked"
            );
            if mode == AnchorMode::Require {
                anyhow::bail!("{message}");
            }
            warn_once(&format!("WARNING: {message}"));
            return Ok(());
        }
    };
    match verify(project_root, &identities) {
        Ok(AttestationStatus::Valid) => Ok(()),
        Ok(AttestationStatus::Absent) => {
            if mode == AnchorMode::Require {
                anyhow::bail!(
                    "no core attestation for '{}' (core '{core}') — run `mgc trust anchor` to attest this project (or set MGC_TRUST_ANCHOR=warn to downgrade)",
                    project_root.display()
                );
            }
            warn_once(&format!(
                "WARNING: no core attestation for '{}' (core '{core}') — run `mgc trust anchor` to bind this project's core identity",
                project_root.display()
            ));
            Ok(())
        }
        Ok(AttestationStatus::Mismatch(reason)) => {
            if mode == AnchorMode::Require {
                anyhow::bail!(
                    "core attestation mismatch for '{}' (core '{core}'): {reason} — run `mgc trust anchor --re-attest` after intentional changes",
                    project_root.display()
                );
            }
            warn_once(&format!(
                "WARNING: core attestation mismatch for '{core}' at '{}': {reason}",
                project_root.display()
            ));
            Ok(())
        }
        Err(error) => {
            if mode == AnchorMode::Require {
                anyhow::bail!("cannot verify core attestation ({error})");
            }
            warn_once(&format!(
                "WARNING: cannot verify core attestation ({error}) — proceeding without anchor"
            ));
            Ok(())
        }
    }
}

static WARNED_ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();

fn warn_once(message: &str) {
    if WARNED_ONCE.set(()).is_ok() {
        eprintln!("{message}");
    }
}
