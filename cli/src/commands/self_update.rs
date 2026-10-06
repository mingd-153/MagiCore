//! `mgc self-update [--version X] [--variant all|web]` — replace the
//! running binary with a verified release artifact (competitor parity:
//! pnpm self-update, bun upgrade, deno upgrade).
//! (Tự nâng cấp binary đang chạy bằng artifact release đã verify.)
//!
//! Fail-closed chain (no step skipped, no silent fallback):
//! resolve version → contract name → download → verify .sha256 →
//! extract → swap with backup → verify new `--version` → drop backup.
//! Any failure aborts BEFORE touching the running binary, except the
//! final swap which always keeps a rollback backup until verified.
//! (Chuỗi fail-closed: lỗi ở bước nào dừng ở đó; chỉ swap khi đã verify.)

use anyhow::Result;

/// GitHub repo hosting the release artifacts.
const RELEASE_REPO: &str = "mingd-153/MagiCore";

/// Resolve the target version: explicit `--version` (with or without a
/// leading `v`), or `latest` via the releases/latest redirect (no JSON
/// parser needed — the effective URL ends in `/tag/<tag>`).
/// (Resolve version: tường minh hoặc `latest` qua redirect.)
pub async fn resolve_version(version: Option<&str>) -> Result<String> {
    match version {
        // Explicit "latest" keyword resolves like the default.
        // (Từ khóa "latest" tường minh resolve như mặc định.)
        Some("latest") | None => resolve_latest_tag().await,
        Some(v) => {
            let number = v.strip_prefix('v').unwrap_or(v);
            if !is_valid_version(number) {
                return Err(crate::error::self_update_invalid_version(v));
            }
            Ok(number.to_string())
        }
    }
}

/// Anchored semver check shared by explicit versions (same rule as the
/// install scripts — unanchored patterns accept `1.2.3evil`).
/// (Kiểm tra semver neo 2 đầu.)
fn is_valid_version(v: &str) -> bool {
    let mut parts = v.splitn(2, '-');
    let core = parts.next().unwrap_or("");
    let nums: Vec<&str> = core.split('.').collect();
    if nums.len() != 3
        || nums
            .iter()
            .any(|n| n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    match parts.next() {
        None => true,
        Some(pre) => {
            !pre.is_empty()
                && pre
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        }
    }
}

/// Follow `/releases/latest` to the newest published tag using the
/// shared HTTP client (timeouts + retries, never raw reqwest).
/// (Theo redirect latest bằng HTTP client chung.)
async fn resolve_latest_tag() -> Result<String> {
    let client = mgc_http::HttpClient::default();
    let url = format!("https://github.com/{RELEASE_REPO}/releases/latest");
    let response = client
        .get(&url)
        .await
        .map_err(|e| anyhow::anyhow!("resolve latest release at {url} failed: {e}"))?;
    let effective = response.url().to_string();
    match effective.rsplit("/tag/").next() {
        Some(tag) if tag != effective && !tag.is_empty() && tag.contains('.') => {
            Ok(tag.trim_start_matches('v').to_string())
        }
        _ => anyhow::bail!(
            "could not resolve the latest release (no published releases at {url}) — re-run with an explicit --version"
        ),
    }
}

/// This host in contract labels (`linux|macos|windows`, `x64|arm64`).
/// Unknown platforms fail closed with the supported list.
/// (OS/arch máy này theo nhãn contract.)
pub fn host_labels() -> Result<(String, String, String)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => {
            return Err(crate::error::self_update_unsupported_host(&format!(
                "OS {other}"
            )));
        }
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => {
            return Err(crate::error::self_update_unsupported_host(&format!(
                "arch {other}"
            )));
        }
    };
    let ext = if os == "windows" { "zip" } else { "tar.gz" };
    Ok((os.to_string(), arch.to_string(), ext.to_string()))
}

/// Artifact file name — the SAME rule as
/// scripts/release-artifact-contract.sh (`{pkg}-{version}-{os}-{arch}.{ext}`,
/// all lowercase, version without `v`). Duplicated here (like install.ps1
/// / install-from-gh.sh) because an installed binary has no repo checkout
/// to call the script from; unit tests pin sample outputs to the contract.
/// (Tên artifact — cùng quy tắc với contract script.)
pub fn artifact_name(package: &str, version: &str, os: &str, arch: &str, ext: &str) -> String {
    format!("{package}-{version}-{os}-{arch}.{ext}")
}

/// Entry point: `mgc self-update [--version X] [--variant all|web] [--dry-run]`.
/// (Điểm vào lệnh self-update.)
/// Maximum release archive accepted (512 MiB — real artifacts are
/// ~50 MiB; anything larger aborts before RAM/disk exhaustion).
/// Quota archive tối đa (chống decompression bomb / nhồi RAM).
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

/// Maximum manifest/signature/checksum document (64 KiB — real ones are
/// hundreds of bytes).
/// Quota document nhỏ.
const MAX_DOC_BYTES: u64 = 64 * 1024;

/// Release manifest: binds every artifact of one release to its
/// coordinates + digests. Published as `manifest.json` next to the
/// archives; `manifest.json.sig` carries the detached Ed25519 signature
/// over the exact manifest bytes. Field-by-field binding is verified —
/// a manifest for another version (or a tampered entry) aborts.
/// (Manifest release: bind mọi artifact với tọa độ + digest.)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReleaseManifest {
    /// Release version WITHOUT leading `v` (e.g. `1.1.0-rc.9`).
    pub version: String,
    /// One entry per published archive.
    pub artifacts: Vec<ManifestArtifact>,
}

/// One manifest entry — all coordinates needed to bind an artifact.
/// (Một entry manifest.)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestArtifact {
    /// Package name (`magicore` | `magicore-web`).
    pub package: String,
    /// Contract OS label (`linux` | `macos` | `windows`).
    pub os: String,
    /// Contract arch label (`x64` | `arm64`).
    pub arch: String,
    /// Archive file name (contract format).
    pub archive: String,
    /// Lowercase hex SHA-256 of the archive bytes.
    pub sha256: String,
    /// Archive size in bytes (quota + truncation check).
    pub size: u64,
}

/// Trust roots for manifest signatures: explicit `--trust-root` flags
/// (repeatable, hex Ed25519 pubkeys) plus `MGC_RELEASE_TRUST_ROOTS`
/// (comma-separated). Empty = no provenance verification (loud warning,
/// sha256-only — the transitional level, never claimed as verified).
/// (Trust roots cho chữ ký manifest.)
pub fn trust_roots(extra: &[String]) -> Vec<String> {
    merge_roots(
        extra,
        std::env::var("MGC_RELEASE_TRUST_ROOTS").ok().as_deref(),
    )
}

/// Pure merge (unit-tested; env reading stays at the boundary).
/// (Gộp thuần để test được.)
fn merge_roots(extra: &[String], env: Option<&str>) -> Vec<String> {
    let mut roots: Vec<String> = extra.to_vec();
    if let Some(env) = env {
        roots.extend(
            env.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        );
    }
    roots
}

/// Resolved provenance policy (P0/F2 — fail-closed):
/// - trust roots configured → Verified. Any unsigned downgrade fails,
///   and combining `--allow-unsigned` with roots is refused outright
///   (a pinned user must never silently stop verifying).
/// - no roots + explicit `--allow-unsigned` → UnsignedWarn: the single
///   escape hatch for legacy-unsigned releases, always loud.
/// - no roots, no flag → hard error prompting a choice (configure a
///   trust root, or pass `--allow-unsigned` alone).
///
/// (Chính sách provenance fail-closed: có root → Verified; không root +
/// cờ tường minh → UnsignedWarn ồn ào; còn lại lỗi cứng.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceMode {
    Verified,
    UnsignedWarn,
}

pub fn resolve_provenance_mode(roots: &[String], allow_unsigned: bool) -> Result<ProvenanceMode> {
    if !roots.is_empty() && allow_unsigned {
        return Err(crate::error::self_update_unsigned_refused(
            "trust roots are configured — refusing --allow-unsigned (unset --trust-root / MGC_RELEASE_TRUST_ROOTS to update from a legacy-unsigned release)",
        ));
    }
    if !roots.is_empty() {
        return Ok(ProvenanceMode::Verified);
    }
    if allow_unsigned {
        return Ok(ProvenanceMode::UnsignedWarn);
    }
    Err(crate::error::self_update_unsigned_refused(
        "no trust roots configured and --allow-unsigned not passed — refusing sha256-only update (pin a key with --trust-root, or pass --allow-unsigned alone for a legacy-unsigned release)",
    ))
}

pub async fn run(
    version: Option<String>,
    variant: Option<String>,
    dry_run: bool,
    trust_root: Vec<String>,
    allow_unsigned: bool,
) -> Result<()> {
    // Explicit --variant wins; otherwise detect from the compiled feature
    // set (a wrong variant 404s, so never guess silently).
    // (--variant tường minh thắng; không thì detect lúc compile.)
    let package = match variant.as_deref() {
        Some("magicore-web") => "magicore-web".to_string(),
        Some("magicore") | Some("all") => "magicore".to_string(),
        Some(other) => return Err(crate::error::self_update_unsupported_variant(other)),
        None => detect_variant(),
    };
    let version_number = resolve_version(version.as_deref()).await?;
    let current = env!("CARGO_PKG_VERSION");
    if version_number == current && !dry_run {
        mgc_ui::info(&format!("already at mgc {current} — nothing to do"));
        return Ok(());
    }
    let (os, arch, ext) = host_labels()?;
    let archive = artifact_name(&package, &version_number, &os, &arch, &ext);
    let base_url = format!("https://github.com/{RELEASE_REPO}/releases/download/v{version_number}");
    let download_url = format!("{base_url}/{archive}");
    let checksum_url = format!("{download_url}.sha256");
    let manifest_url = format!("{base_url}/manifest.json");
    let manifest_sig_url = format!("{base_url}/manifest.json.sig");
    if dry_run {
        mgc_ui::info(&format!(
            "would download {download_url} (+ .sha256, + manifest.json)"
        ));
        return Ok(());
    }
    // Fail fast on provenance policy (bad flag combinations error before
    // any network, not after a half-finished update).
    // (Chốt policy provenance trước mọi download.)
    let roots = trust_roots(&trust_root);
    let mode = resolve_provenance_mode(&roots, allow_unsigned)?;
    mgc_ui::info(&format!("downloading mgc {version_number} ({archive})..."));
    let tmp = tempfile::tempdir().map_err(|e| anyhow::anyhow!("create temp dir: {e}"))?;
    let archive_path = tmp.path().join(&archive);
    download_file(&download_url, &archive_path).await?;
    let checksum_text = download_text(&checksum_url).await?;
    verify_checksum(&archive_path, &checksum_text)?;
    // Release-manifest provenance: every field must bind THIS exact
    // target, and the digest must match the bytes just verified. A
    // manifest for another version/variant/platform aborts here.
    // (Manifest phải bind đúng target này.)
    verify_manifest(
        &manifest_url,
        &manifest_sig_url,
        &version_number,
        &package,
        &os,
        &arch,
        &archive,
        &archive_path,
        &roots,
        mode,
    )
    .await?;
    let (staging_dir, staged) = stage_binary(&archive_path, &ext)?;
    let swap_result = swap_binary(&staged, &version_number).await;
    // Staging always cleaned (success AND failure) — keep() exists only
    // to outlive the extraction function, not the command.
    // (Staging luôn dọn — thành công hay thất bại.)
    let _ = std::fs::remove_dir_all(&staging_dir);
    swap_result?;
    mgc_ui::success(&format!("updated to mgc {version_number}"));
    Ok(())
}

/// Which release variant this binary is: `all` when every core feature
/// is compiled in, else `web`. Determined at compile time, never guessed
/// at runtime (a wrong variant 404s).
/// Which release variant this binary is: `all` when every core feature
/// is compiled in, else `web`. Determined at compile time, never guessed
/// at runtime (a wrong variant 404s).
/// (Variant của binary này — xác định lúc compile.)
fn detect_variant() -> String {
    #[cfg(all(
        feature = "web",
        feature = "game",
        feature = "ai",
        feature = "clo",
        feature = "cicd",
        feature = "iot",
        feature = "app",
        feature = "lib",
        feature = "hardware"
    ))]
    {
        "magicore".to_string()
    }
    #[cfg(not(all(
        feature = "web",
        feature = "game",
        feature = "ai",
        feature = "clo",
        feature = "cicd",
        feature = "iot",
        feature = "app",
        feature = "lib",
        feature = "hardware"
    )))]
    {
        "magicore-web".to_string()
    }
}

/// Download a URL to a path with the shared client, STREAMING with a
/// byte quota (never buffers the whole body in RAM — a hostile
/// oversized archive aborts mid-stream instead of exhausting memory).
/// (Tải URL ra file dạng stream có quota.)
async fn download_file(url: &str, dest: &std::path::Path) -> Result<()> {
    download_stream(url, dest, MAX_ARCHIVE_BYTES).await
}

/// Download a URL as text (checksums, manifests — small documents with
/// a tight quota).
/// (Tải URL dạng text.)
async fn download_text(url: &str) -> Result<String> {
    download_capped(url, MAX_DOC_BYTES).await.and_then(|bytes| {
        String::from_utf8(bytes)
            .map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))
    })
}

async fn download_capped(url: &str, quota: u64) -> Result<Vec<u8>> {
    let client = mgc_http::HttpClient::default();
    let response = client
        .get(url)
        .await
        .map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(crate::error::self_update_download_failed(
            url,
            &format!("HTTP {status}"),
        ));
    }
    if let Some(len) = response.content_length()
        && len > quota
    {
        return Err(crate::error::self_update_download_failed(
            url,
            &format!("declared {len} bytes over quota {quota}"),
        ));
    }
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
        if (buf.len() as u64) + (chunk.len() as u64) > quota {
            return Err(crate::error::self_update_download_failed(
                url,
                &format!("body over quota {quota}"),
            ));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

async fn download_stream(url: &str, dest: &std::path::Path, quota: u64) -> Result<()> {
    let client = mgc_http::HttpClient::default();
    let response = client
        .get(url)
        .await
        .map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(crate::error::self_update_download_failed(
            url,
            &format!("HTTP {status}"),
        ));
    }
    if let Some(len) = response.content_length()
        && len > quota
    {
        return Err(crate::error::self_update_download_failed(
            url,
            &format!("declared {len} bytes over quota {quota}"),
        ));
    }
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
    let mut out = tokio::io::BufWriter::new(file);
    let mut written: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
        written += chunk.len() as u64;
        if written > quota {
            return Err(crate::error::self_update_download_failed(
                url,
                &format!("body over quota {quota}"),
            ));
        }
        out.write_all(&chunk)
            .await
            .map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
    }
    out.flush()
        .await
        .map_err(|e| crate::error::self_update_download_failed(url, &e.to_string()))?;
    Ok(())
}

/// Hash a file with SHA-256 in 64 KiB streaming reads — the 512 MiB
/// archive is NEVER fully buffered in RAM (P1: the old read-all-then-hash
/// doubled peak memory on top of the download staging).
/// (Hash file dạng stream — archive không bao giờ nằm trọn trong RAM.)
fn sha256_file_streaming(path: &std::path::Path) -> Result<(u64, String)> {
    use sha2::Digest;
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut total: u64 = 0;
    let mut chunk = [0u8; 65536];
    loop {
        let n = file
            .read(&mut chunk)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hasher.update(&chunk[..n]);
    }
    Ok((total, hex::encode(hasher.finalize())))
}

/// Verify `sha256  filename` checksum text against the file. Mismatch =
/// delete nothing (caller temp dir cleans itself) + hard error BEFORE
/// any extract or swap.
/// (Verify checksum — lệch là fail cứng trước mọi bước sau.)
fn verify_checksum(archive_path: &std::path::Path, checksum_text: &str) -> Result<()> {
    let expected = checksum_text
        .split_whitespace()
        .next()
        .ok_or_else(|| {
            crate::error::self_update_download_failed(
                &archive_path.display().to_string(),
                "checksum file is empty",
            )
        })?
        .to_lowercase();
    let (_, actual) = sha256_file_streaming(archive_path)?;
    if actual != expected {
        return Err(crate::error::self_update_checksum_mismatch(
            &archive_path.display().to_string(),
            &expected,
            &actual,
        ));
    }
    Ok(())
}

/// Fetch + verify the release manifest, then bind it to this exact
/// download. Every field must match the requested target; the digest
/// must match the verified bytes; the size must match too (truncation
/// check independent of the streaming quota).
/// (Fetch + verify manifest — mọi field phải khớp target này.)
#[allow(clippy::too_many_arguments)]
async fn verify_manifest(
    manifest_url: &str,
    manifest_sig_url: &str,
    version_number: &str,
    package: &str,
    os: &str,
    arch: &str,
    archive: &str,
    archive_path: &std::path::Path,
    trust_root: &[String],
    mode: ProvenanceMode,
) -> Result<()> {
    let manifest_bytes = match download_capped(manifest_url, MAX_DOC_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) => {
            // P0/F2: a manifest fetch failure is NEVER silently treated
            // as "legacy release" — DNS/TLS/500 and 404 all land here.
            // Verified mode fails closed; only the explicit unsigned
            // opt-in (no roots) proceeds sha256-only, LOUDLY.
            // (Lỗi tải manifest không bao giờ âm thầm thành "release cũ".)
            if mode == ProvenanceMode::Verified {
                return Err(crate::error::self_update_manifest_invalid(&format!(
                    "manifest download failed under configured trust roots: {e}"
                )));
            }
            mgc_ui::warning(
                "no release manifest published for this version — provenance UNVERIFIED (sha256 transport check only; explicit --allow-unsigned)",
            );
            return Ok(());
        }
    };
    let manifest_text = String::from_utf8(manifest_bytes.clone()).map_err(|e| {
        crate::error::self_update_manifest_invalid(&format!("manifest is not UTF-8: {e}"))
    })?;
    let manifest: ReleaseManifest = serde_json::from_str(&manifest_text)
        .map_err(|e| crate::error::self_update_manifest_invalid(&e.to_string()))?;
    // The manifest must be FOR this release; then select OUR entry.
    // (Manifest phải đúng release này; rồi chọn entry của target.)
    if manifest.version != version_number {
        return Err(crate::error::self_update_manifest_invalid(&format!(
            "manifest version {} != {version_number}",
            manifest.version
        )));
    }
    let entry = manifest
        .artifacts
        .iter()
        .find(|entry| {
            entry.package == package
                && entry.os == os
                && entry.arch == arch
                && entry.archive == archive
        })
        .ok_or_else(|| {
            crate::error::self_update_manifest_invalid(&format!(
                "no manifest entry binds {archive} (manifest covers {} artifact(s))",
                manifest.artifacts.len()
            ))
        })?;
    // Digest + size bind the manifest entry to the exact bytes on disk
    // (streaming: size and digest in one pass, no full RAM buffering).
    // (Digest + size bind entry với byte đã tải — stream một lượt.)
    let (actual_size, actual_digest) = sha256_file_streaming(archive_path)?;
    if actual_size != entry.size {
        return Err(crate::error::self_update_manifest_invalid(&format!(
            "size {actual_size} != manifest {}",
            entry.size
        )));
    }
    if actual_digest != entry.sha256.to_lowercase() {
        return Err(crate::error::self_update_manifest_invalid(
            "digest mismatch between manifest and downloaded bytes",
        ));
    }
    verify_manifest_signature(manifest_sig_url, &manifest_bytes, trust_root, mode).await
}

/// Verify the detached manifest signature against trust roots. Verified
/// mode fails closed on ANY signature problem (missing or invalid);
/// only the explicit unsigned opt-in (no roots) proceeds sha256-only,
/// LOUDLY. A PRESENT but invalid signature always fails, keys or not.
/// (Verify chữ ký — Verified fail-closed mọi lỗi; chỉ opt-in tường minh
/// mới đi tiếp sha256-only.)
async fn verify_manifest_signature(
    manifest_sig_url: &str,
    manifest_bytes: &[u8],
    trust_root: &[String],
    mode: ProvenanceMode,
) -> Result<()> {
    let roots = trust_roots(trust_root);
    let sig_bytes = match download_capped(manifest_sig_url, MAX_DOC_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) if mode == ProvenanceMode::Verified => {
            return Err(crate::error::self_update_signature_invalid(&format!(
                "trust roots configured but signature download failed: {e}"
            )));
        }
        Err(_) => {
            mgc_ui::warning(
                "no manifest signature published and no trust roots configured — provenance UNVERIFIED (sha256 transport check only; explicit --allow-unsigned)",
            );
            return Ok(());
        }
    };
    if roots.is_empty() {
        if mode == ProvenanceMode::Verified {
            return Err(crate::error::self_update_signature_invalid(
                "manifest signature present but trust-root resolution is inconsistent — refusing unsigned verification",
            ));
        }
        mgc_ui::warning(
            "manifest signature present but no trust roots configured (--trust-root or MGC_RELEASE_TRUST_ROOTS) — provenance UNVERIFIED (explicit --allow-unsigned)",
        );
        return Ok(());
    }
    let sig_text = String::from_utf8(sig_bytes)
        .map_err(|e| crate::error::self_update_signature_invalid(&e.to_string()))?;
    let sig_hex = sig_text
        .split_whitespace()
        .next()
        .ok_or_else(|| crate::error::self_update_signature_invalid("empty signature file"))?;
    let sig_bytes = hex::decode(sig_hex)
        .map_err(|e| crate::error::self_update_signature_invalid(&e.to_string()))?;
    let signature = mgc_crypto::ed25519_signer::Ed25519Signature(sig_bytes);
    for root in &roots {
        let key_bytes = hex::decode(root.trim()).map_err(|e| {
            crate::error::self_update_signature_invalid(&format!("bad trust-root hex: {e}"))
        })?;
        let key = mgc_crypto::ed25519_signer::Ed25519PublicKey(key_bytes);
        if mgc_crypto::ed25519_signer::verify_signature(&key, manifest_bytes, &signature).is_ok() {
            return Ok(());
        }
    }
    Err(crate::error::self_update_signature_invalid(
        "no configured trust root verifies this manifest",
    ))
}

/// Extract the single `mgc`/`mgc.exe` binary from the archive into a
/// fresh temp dir. Rejects path traversal inside the archive (zip-slip /
/// tar-slip): entries escaping the staging dir fail closed.
/// (Bung binary khỏi archive — chặn path traversal trong archive.)
fn stage_binary(
    archive_path: &std::path::Path,
    ext: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let staging = tempfile::tempdir().map_err(|e| anyhow::anyhow!("create staging dir: {e}"))?;
    // The staging dir outlives this function (caller cleans it after a
    // successful swap AND on every failure path).
    // (Staging dir sống qua function — caller dọn mọi đường.)
    let staging_path = staging.keep();
    let found = if ext == "zip" {
        stage_from_zip(archive_path, &staging_path)?
    } else {
        stage_from_tar_gz(archive_path, &staging_path)?
    };
    match found {
        Some(binary) => Ok((staging_path, binary)),
        None => Err(crate::error::self_update_no_binary(
            &archive_path.display().to_string(),
        )),
    }
}

/// Pick the `mgc`/`mgc.exe` REGULAR file out of a zip (symlinks,
/// directories, and traversal entries can never match).
/// (Chọn file regular mgc trong zip.)
fn stage_from_zip(
    archive_path: &std::path::Path,
    staging_path: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| anyhow::anyhow!("open {}: {e}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| anyhow::anyhow!("read zip {}: {e}", archive_path.display()))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| anyhow::anyhow!("read zip entry {i}: {e}"))?;
        if !entry.is_file() {
            continue;
        }
        let Some(safe_path) = entry.enclosed_name() else {
            continue;
        };
        let file_name = safe_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("zip entry has no file name"))?;
        if file_name == "mgc" || file_name == "mgc.exe" {
            let dest = staging_path.join(file_name);
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| anyhow::anyhow!("create {}: {e}", dest.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| anyhow::anyhow!("extract {}: {e}", dest.display()))?;
            set_executable(&dest)?;
            return Ok(Some(dest));
        }
    }
    Ok(None)
}

/// Pick the `mgc`/`mgc.exe` REGULAR file out of a tar.gz.
/// (Chọn file regular mgc trong tar.gz.)
fn stage_from_tar_gz(
    archive_path: &std::path::Path,
    staging_path: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| anyhow::anyhow!("open {}: {e}", archive_path.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    for entry in archive
        .entries()
        .map_err(|e| anyhow::anyhow!("read tar {}: {e}", archive_path.display()))?
    {
        let mut entry = entry.map_err(|e| anyhow::anyhow!("read tar entry: {e}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|e| anyhow::anyhow!("read tar entry path: {e}"))?;
        let Some(file_name) = path.file_name() else {
            continue;
        };
        if file_name == "mgc" || file_name == "mgc.exe" {
            let dest = staging_path.join(file_name);
            entry
                .unpack(&dest)
                .map_err(|e| anyhow::anyhow!("extract {}: {e}", dest.display()))?;
            set_executable(&dest)?;
            return Ok(Some(dest));
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta =
        std::fs::metadata(path).map_err(|e| anyhow::anyhow!("stat {}: {e}", path.display()))?;
    let mut perms = meta.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .map_err(|e| anyhow::anyhow!("chmod {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

/// Swap the running binary with the staged one, keeping a rollback
/// backup until the new binary proves itself (`--version` exit 0).
/// Windows: a running exe cannot be overwritten, but it CAN be renamed
/// aside — so rename-then-move works on all three OSes.
/// (Thay binary đang chạy, giữ backup rollback tới khi verify xong.)
/// Swap the running binary with the staged one, keeping a rollback
/// backup until the new binary proves itself: it must launch (exit 0)
/// AND report EXACTLY the requested version (`mgc <version>` token). A
/// wrong-version artifact is rolled back, never kept with the backup
/// deleted. Windows: a running exe cannot be overwritten, but it CAN be
/// renamed aside — so rename-then-move works on all three OSes.
/// (Thay binary, giữ backup tới khi probe khớp version chính xác.)
async fn swap_binary(staged: &std::path::Path, version_number: &str) -> Result<()> {
    let current =
        std::env::current_exe().map_err(|e| anyhow::anyhow!("locate running binary: {e}"))?;
    let backup = current.with_extension("mgc-backup");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(&current, &backup)
        .map_err(|e| anyhow::anyhow!("back up running binary: {e}"))?;
    if let Err(e) = std::fs::rename(staged, &current) {
        let _ = std::fs::rename(&backup, &current);
        return Err(anyhow::anyhow!("install new binary (rolled back): {e}"));
    }
    // Prove the new binary launches AND reports the requested version —
    // spawn by absolute path (project-binary guardrails: canonicalized
    // file). stdout_tail carries `--version` output for the exact check.
    // (Chứng minh binary mới chạy được VÀ đúng version yêu cầu.)
    let probe = mgc_exec::prelude::run_project_binary(
        &current,
        &["--version".to_string()],
        &mgc_exec::prelude::ExecOptions {
            ..Default::default()
        },
    );
    let rollback = |detail: String| {
        let _ = std::fs::rename(&backup, &current);
        crate::error::self_update_probe_failed(&detail)
    };
    let report = match probe {
        Ok(report) if report.exit_code == 0 => report,
        Ok(report) => {
            return Err(rollback(format!("exit code {}", report.exit_code)));
        }
        Err(e) => {
            return Err(rollback(format!("did not launch: {e}")));
        }
    };
    let expected = format!("mgc {version_number}");
    // The probe output must contain the requested version as a
    // whitespace-delimited token (`mgc 1.1.0-rc.9`): substring matching
    // would accept `1.1.0-rc.90`.
    // (Output probe phải chứa đúng version yêu cầu.)
    if !probe_reports_version(&report.stdout_tail, version_number) {
        return Err(rollback(format!(
            "reported {:?} instead of {expected}",
            report.stdout_tail.trim()
        )));
    }
    let _ = std::fs::remove_file(&backup);
    Ok(())
}

/// True when `--version` output contains the requested version as a
/// whitespace-delimited token. Pure function so the exact-match rule is
/// unit-tested (a wrong-version artifact must never survive the probe).
/// (Output chứa đúng version — hàm thuần để test.)
fn probe_reports_version(stdout: &str, version_number: &str) -> bool {
    stdout
        .split_whitespace()
        .any(|token| token == version_number)
}

#[cfg(test)]
#[path = "test/self_update.rs"]
mod tests;
