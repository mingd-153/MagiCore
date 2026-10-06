//! Verify registry-signed package provenance and its per-package hash chain.
//! Xác minh provenance do registry ký và chuỗi hash theo từng package.

use anyhow::Result;
use mgc_crypto::ed25519_signer::verify_signature;
use mgc_crypto::{Ed25519PublicKey, Ed25519Signature};
use mgc_registry_server::trusted::AttestationBundle;
use sha2::{Digest, Sha256};
use sigstore_types::{Bundle as SigstoreBundle, Sha256Hash, SignatureContent, Statement};
use sigstore_verify::{VerificationPolicy, Verifier};
use std::time::Duration;

const MAX_SIGNATURE_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 15;

/// Add a canonical core namespace when auditing a trusted OCI artifact repository.
/// Thêm namespace core chuẩn khi audit repository artifact OCI trusted.
fn effective_signature_package(
    core: Option<&str>,
    protocol: &str,
    package: &str,
) -> Result<String> {
    match (protocol, core) {
        ("oci", Some(core)) => {
            mgc_registry_server::trusted::core_scoped_oci_repository(core, package)
                .map_err(|_| crate::error::trusted_signatures_invalid())
        }
        _ => Ok(package.to_owned()),
    }
}

pub async fn run(
    core: Option<&str>,
    package: &str,
    protocol: &str,
    registry: &str,
    token: Option<&str>,
    json: bool,
) -> Result<()> {
    let effective_package = effective_signature_package(core, protocol, package)?;
    let package = effective_package.as_str();
    let scoped_package = mgc_registry_server::trusted::scoped_package(protocol, package)
        .map_err(|_| crate::error::trusted_signatures_invalid())?;
    let mut registry_url =
        url::Url::parse(registry).map_err(|_| crate::error::trusted_registry_url_invalid())?;
    if !registry_url.username().is_empty()
        || registry_url.password().is_some()
        || registry_url.fragment().is_some()
        || registry_url.query().is_some()
    {
        return Err(crate::error::trusted_registry_url_invalid());
    }
    let local_http = registry_url.scheme() == "http"
        && registry_url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    if registry_url.scheme() != "https" && !local_http {
        return Err(crate::error::trusted_registry_requires_https());
    }
    let prefix = registry_url.path().trim_end_matches('/');
    registry_url.set_path(&format!("{prefix}/-/v1/trusted-publish/signatures"));

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()?;
    let mut after_sequence = 0u64;
    let mut snapshot_head: Option<u64> = None;
    let mut previous_hash: Option<String> = None;
    let mut public_key: Option<Ed25519PublicKey> = None;
    let mut key_id: Option<String> = None;
    let mut verified_count = 0u64;
    let mut public_verified_count = 0u64;
    let mut legacy_internal_only_count = 0u64;
    let mut sigstore_verifier: Option<Verifier> = None;
    loop {
        let mut page_url = registry_url.clone();
        page_url.set_query(None);
        {
            let mut query = page_url.query_pairs_mut();
            query
                .append_pair("package", package)
                .append_pair("protocol", protocol)
                .append_pair("after_sequence", &after_sequence.to_string())
                .append_pair(
                    "limit",
                    &mgc_registry_server::trusted::SIGNATURE_PAGE_SIZE.to_string(),
                );
            if let Some(head) = snapshot_head {
                query.append_pair("through_sequence", &head.to_string());
            }
        }
        let mut request = client.get(page_url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(crate::error::trusted_signatures_request_failed(
                response.status().as_u16(),
            ));
        }
        let page = read_bundle(response).await?;
        if page.package != scoped_package
            || page.entries.len() > mgc_registry_server::trusted::SIGNATURE_PAGE_SIZE
        {
            return Err(crate::error::trusted_signatures_invalid());
        }
        if page.head_sequence == 0 {
            return Err(crate::error::trusted_signatures_missing_or_mismatched());
        }
        if snapshot_head.is_some_and(|head| head != page.head_sequence) {
            return Err(crate::error::trusted_signatures_invalid());
        }
        snapshot_head = Some(page.head_sequence);

        let page_key = verified_bundle_key(&page)?;
        if public_key
            .as_ref()
            .is_some_and(|expected: &Ed25519PublicKey| expected != &page_key)
            || key_id
                .as_ref()
                .is_some_and(|expected: &String| expected != &page.key_id)
        {
            return Err(crate::error::trusted_signatures_invalid());
        }
        let expected_sequence = after_sequence
            .checked_add(1)
            .ok_or_else(crate::error::trusted_signatures_invalid)?;
        previous_hash = verify_page(
            &scoped_package,
            protocol,
            &page.entries,
            &page.key_id,
            &page_key,
            expected_sequence,
            previous_hash,
        )?;
        for entry in &page.entries {
            match (&entry.payload.artifact_sha256, &entry.sigstore_bundle) {
                (Some(_), Some(bundle)) => {
                    if sigstore_verifier.is_none() {
                        let trusted_root = tokio::time::timeout(
                            Duration::from_secs(60),
                            sigstore_trust_root::TrustedRoot::production(),
                        )
                        .await
                        .map_err(|_| anyhow::anyhow!("Sigstore trust-root fetch timed out"))??;
                        sigstore_verifier = Some(Verifier::new(&trusted_root));
                    }
                    verify_public_attestation(
                        entry,
                        bundle,
                        sigstore_verifier
                            .as_ref()
                            .ok_or_else(crate::error::trusted_signatures_invalid)?,
                    )?;
                    public_verified_count = public_verified_count
                        .checked_add(1)
                        .ok_or_else(crate::error::trusted_signatures_invalid)?;
                }
                (None, None) => {
                    legacy_internal_only_count = legacy_internal_only_count
                        .checked_add(1)
                        .ok_or_else(crate::error::trusted_signatures_invalid)?;
                }
                _ => return Err(crate::error::trusted_signatures_invalid()),
            }
        }
        public_key = Some(page_key);
        key_id = Some(page.key_id);
        verified_count = verified_count
            .checked_add(page.entries.len() as u64)
            .ok_or_else(crate::error::trusted_signatures_invalid)?;
        if let Some(last_entry) = page.entries.last() {
            after_sequence = last_entry.payload.sequence;
        }
        if after_sequence == page.head_sequence {
            break;
        }
        if page.entries.is_empty() || after_sequence > page.head_sequence {
            return Err(crate::error::trusted_signatures_invalid());
        }
    }
    let key_id = key_id.ok_or_else(crate::error::trusted_signatures_missing_or_mismatched)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "package": package,
                "protocol": protocol,
                "verified": true,
                "entry_count": verified_count,
                "public_sigstore_verified_count": public_verified_count,
                "legacy_internal_only_count": legacy_internal_only_count,
                "head_sequence": snapshot_head,
                "key_id": key_id,
            }))?
        );
    } else {
        println!(
            "Verified {verified_count} {protocol} provenance entries for {package} (registry key {key_id}); {public_verified_count} have public Sigstore verification, {legacy_internal_only_count} are legacy registry-only entries."
        );
    }
    Ok(())
}

async fn read_bundle(response: reqwest::Response) -> Result<AttestationBundle> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SIGNATURE_BUNDLE_BYTES as u64)
    {
        return Err(crate::error::trusted_signatures_response_too_large());
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| crate::error::trusted_signatures_response_invalid())?;
        if chunk.len() > MAX_SIGNATURE_BUNDLE_BYTES.saturating_sub(body.len()) {
            return Err(crate::error::trusted_signatures_response_too_large());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| crate::error::trusted_signatures_response_invalid())
}

fn verified_bundle_key(bundle: &AttestationBundle) -> Result<Ed25519PublicKey> {
    let public_key = Ed25519PublicKey::from_base64(&bundle.public_key)
        .map_err(|_| crate::error::trusted_signatures_invalid())?;
    let key_hash = mgc_crypto::Blake3Hasher::hash_bytes(&public_key.0);
    let expected_key_id = hex::encode(&key_hash.0[..8]);
    if expected_key_id != bundle.key_id {
        return Err(crate::error::trusted_signatures_invalid());
    }
    Ok(public_key)
}

fn verify_page(
    package: &str,
    protocol: &str,
    entries: &[mgc_registry_server::trusted::SignedAttestation],
    key_id: &str,
    public_key: &Ed25519PublicKey,
    first_sequence: u64,
    mut previous_hash: Option<String>,
) -> Result<Option<String>> {
    for (index, entry) in entries.iter().enumerate() {
        let sequence = first_sequence
            .checked_add(index as u64)
            .ok_or_else(crate::error::trusted_signatures_invalid)?;
        if entry.payload.package != package
            || entry.payload.schema_version != 1
            || entry.payload.sequence != sequence
            || entry.payload.previous_hash != previous_hash
            || entry.payload.builder.issuer.is_empty()
            || entry.payload.builder.repository.is_empty()
            || entry.payload.created_at_unix == 0
            || !entry
                .payload
                .builder
                .subject
                .starts_with(&format!("repo:{}:", entry.payload.builder.repository))
            || entry.key_id != key_id
            || match protocol {
                "npm" => {
                    !mgc_registry_server::trusted::is_valid_sha512_integrity(&entry.payload.digest)
                }
                "pypi" | "oci" => entry
                    .payload
                    .artifact_sha256
                    .as_ref()
                    .is_none_or(|digest| entry.payload.digest != format!("sha256:{digest}")),
                _ => true,
            }
        {
            return Err(crate::error::trusted_signatures_invalid());
        }
        let canonical = serde_json::to_vec(&entry.payload)?;
        let calculated_hash = hex::encode(Sha256::digest(canonical));
        if calculated_hash != entry.entry_hash {
            return Err(crate::error::trusted_signatures_invalid());
        }
        let signature = Ed25519Signature::from_base64(&entry.signature)
            .map_err(|_| crate::error::trusted_signatures_invalid())?;
        verify_signature(public_key, entry.entry_hash.as_bytes(), &signature)
            .map_err(|_| crate::error::trusted_signatures_invalid())?;
        previous_hash = Some(entry.entry_hash.clone());
    }
    Ok(previous_hash)
}

#[cfg(test)]
fn verify_bundle(package: &str, bundle: &AttestationBundle) -> Result<usize> {
    if bundle.package != package || bundle.entries.is_empty() {
        return Err(crate::error::trusted_signatures_missing_or_mismatched());
    }
    if bundle.head_sequence != bundle.entries.len() as u64 {
        return Err(crate::error::trusted_signatures_invalid());
    }
    let public_key = verified_bundle_key(bundle)?;
    verify_page(
        package,
        "npm",
        &bundle.entries,
        &bundle.key_id,
        &public_key,
        1,
        None,
    )?;
    Ok(bundle.entries.len())
}

fn verify_public_attestation(
    entry: &mgc_registry_server::trusted::SignedAttestation,
    bundle_json: &serde_json::Value,
    verifier: &Verifier,
) -> Result<()> {
    let bundle = SigstoreBundle::from_json(&serde_json::to_string(bundle_json)?)
        .map_err(|_| crate::error::trusted_signatures_invalid())?;
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err(crate::error::trusted_signatures_invalid());
    };
    if envelope.payload_type != "application/vnd.in-toto+json" {
        return Err(crate::error::trusted_signatures_invalid());
    }
    let statement: Statement = serde_json::from_slice(envelope.payload.as_bytes())
        .map_err(|_| crate::error::trusted_signatures_invalid())?;
    validate_sigstore_statement(&entry.payload, &statement)?;
    let artifact_hash = entry
        .payload
        .artifact_sha256
        .as_deref()
        .filter(|digest| mgc_registry_server::trusted::is_valid_sha256_hex(digest))
        .ok_or_else(crate::error::trusted_signatures_invalid)?;
    let workflow = entry
        .payload
        .builder
        .job_workflow_ref
        .as_deref()
        .or(entry.payload.builder.workflow_ref.as_deref())
        .ok_or_else(crate::error::trusted_signatures_invalid)?;
    let expected_identity = format!("https://github.com/{workflow}");
    let policy = VerificationPolicy::default()
        .require_issuer(entry.payload.builder.issuer.clone())
        .require_identity(expected_identity);
    verifier
        .verify(Sha256Hash::from_hex(artifact_hash)?, &bundle, &policy)
        .map_err(|_| crate::error::trusted_signatures_invalid())?;
    Ok(())
}

fn validate_sigstore_statement(
    payload: &mgc_registry_server::trusted::AttestationPayload,
    statement: &Statement,
) -> Result<()> {
    let artifact_sha256 = payload
        .artifact_sha256
        .as_deref()
        .filter(|digest| mgc_registry_server::trusted::is_valid_sha256_hex(digest))
        .ok_or_else(crate::error::trusted_signatures_invalid)?;
    let expected_builder = serde_json::to_value(&payload.builder)?;
    let predicate = &statement.predicate;
    let subject_matches = statement.subject.iter().any(|subject| {
        !subject.name.is_empty() && subject.digest.sha256.as_deref() == Some(artifact_sha256)
    });
    if statement.type_ != "https://in-toto.io/Statement/v1"
        || statement.predicate_type != mgc_registry_server::trusted::TRUSTED_PUBLISH_PREDICATE_TYPE
        || !subject_matches
        || predicate.get("package").and_then(serde_json::Value::as_str)
            != Some(payload.package.as_str())
        || predicate.get("version").and_then(serde_json::Value::as_str)
            != Some(payload.version.as_str())
        || predicate
            .get("registryDigest")
            .and_then(serde_json::Value::as_str)
            != Some(payload.digest.as_str())
        || predicate.get("builder") != Some(&expected_builder)
    {
        return Err(crate::error::trusted_signatures_invalid());
    }
    Ok(())
}

use futures_util::StreamExt;

#[cfg(test)]
#[path = "test/signatures.rs"]
mod tests;
