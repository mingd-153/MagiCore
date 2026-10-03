//! Trusted publishing routes and signed provenance records.
//! Route trusted publishing và bản ghi provenance có chữ ký.

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
};
use base64::Engine;
use mgc_crypto::{Ed25519Signature, KeyPair};
use mgc_oidc::OidcClaims;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::AppState;
use sha2::Digest;

pub const TRUSTED_TOKEN_TTL_SECS: u64 = 15 * 60;
pub const MAX_ACTIVE_TRUSTED_TOKENS: usize = 10_000;
pub const SIGNATURE_PAGE_SIZE: usize = 128;
pub const MAX_SIGNATURE_PAGE_SIZE: usize = 256;
pub const TRUSTED_PUBLISH_PREDICATE_TYPE: &str =
    "https://github.com/mingd-153/MagiCore/attestations/trusted-publish/v1";
const SIGNING_KEY_FILE: &str = "registry-attestation-key.json";
const MAX_SIGNING_KEY_BYTES: u64 = 16 * 1024;
const MAX_TRUSTED_REQUEST_BYTES: usize = 20 * 1024;
const MAX_SIGSTORE_BUNDLE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct TrustedPublishingConfig {
    pub issuers: Vec<String>,
    pub audience: String,
}

/// Registry protocol used to keep a package binding unique across ecosystems.
/// Giao thức registry giúp binding không bị trộn giữa các hệ sinh thái.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedProtocol {
    Npm,
    Pypi,
    Oci,
}

impl TrustedProtocol {
    pub fn parse(value: &str) -> Result<Self, StatusCode> {
        match value {
            "npm" => Ok(Self::Npm),
            "pypi" => Ok(Self::Pypi),
            "oci" => Ok(Self::Oci),
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pypi => "pypi",
            Self::Oci => "oci",
        }
    }
}

/// Build the storage and authorization key for one ecosystem package.
/// Tạo khóa lưu trữ và phân quyền riêng cho từng hệ sinh thái.
pub fn scoped_package(protocol: &str, package: &str) -> Result<String, StatusCode> {
    match TrustedProtocol::parse(protocol)? {
        TrustedProtocol::Npm => {
            validate_package(package)?;
            Ok(package.to_owned())
        }
        TrustedProtocol::Pypi => {
            let normalized = canonical_pypi_name(package)?;
            Ok(format!("pypi:{normalized}"))
        }
        TrustedProtocol::Oci => {
            validate_oci_repository(package)?;
            Ok(format!("oci:{package}"))
        }
    }
}

/// Namespace one OCI artifact repository under a canonical MagiCore core id.
/// Đặt OCI repository dưới mã core MagiCore chuẩn để trust không chéo core.
pub fn core_scoped_oci_repository(core: &str, package: &str) -> Result<String, StatusCode> {
    let core = mgc_types::Ecosystem::from_str(core).ok_or(StatusCode::BAD_REQUEST)?;
    let repository = format!("{}/{package}", core.as_str());
    validate_oci_repository(&repository)?;
    Ok(repository)
}

/// Match repository bindings while rejecting GitHub pull-request identities.
/// Đối chiếu binding repository và từ chối danh tính pull request của GitHub.
pub fn claims_match_repository_binding(claims: &OidcClaims, repository: &str) -> bool {
    let subject_prefix = format!("repo:{repository}:");
    let Some(subject) = claims.sub.strip_prefix(&subject_prefix) else {
        return false;
    };
    let pull_request_event = claims.event_name.as_deref().is_some_and(|event| {
        matches!(
            event.to_ascii_lowercase().as_str(),
            "pull_request" | "pull_request_target"
        )
    });
    claims.repository.as_deref() == Some(repository)
        && !subject.is_empty()
        && !subject.starts_with("pull_request")
        && !pull_request_event
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuilderIdentity {
    pub issuer: String,
    pub subject: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_workflow_ref: Option<String>,
    pub workflow_ref: Option<String>,
    pub event_name: Option<String>,
}

impl From<&OidcClaims> for BuilderIdentity {
    fn from(claims: &OidcClaims) -> Self {
        Self {
            issuer: claims.iss.clone(),
            subject: claims.sub.clone(),
            repository: claims.repository.clone().unwrap_or_default(),
            job_workflow_ref: claims.job_workflow_ref.clone(),
            workflow_ref: claims.workflow_ref.clone(),
            event_name: claims.event_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttestationPayload {
    pub schema_version: u32,
    pub package: String,
    pub version: String,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<String>,
    pub sequence: u64,
    pub builder: BuilderIdentity,
    pub created_at_unix: u64,
    pub previous_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedAttestation {
    pub payload: AttestationPayload,
    pub entry_hash: String,
    pub key_id: String,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigstore_bundle: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttestationBundle {
    pub package: String,
    pub public_key: String,
    pub key_id: String,
    pub head_sequence: u64,
    pub entries: Vec<SignedAttestation>,
}

/// Load a private signing key without following symlinks, or create it with owner-only permissions.
/// Tải khóa ký riêng không theo symlink, hoặc tạo khóa chỉ chủ sở hữu được đọc.
pub fn load_or_create_signing_key(store_dir: &Path) -> Result<KeyPair> {
    let path = store_dir.join(SIGNING_KEY_FILE);
    match read_signing_key(&path) {
        Ok(Some(key)) => return Ok(key),
        Ok(None) => {}
        Err(error) => return Err(error),
    }

    let key = KeyPair::generate().context("generate registry provenance key")?;
    let serialized = serde_json::to_vec(&key).context("serialize registry provenance key")?;
    let mut temporary = tempfile::NamedTempFile::new_in(store_dir)
        .context("create temporary registry provenance key")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .context("secure temporary registry provenance key")?;
    }
    temporary
        .write_all(&serialized)
        .context("write registry provenance key")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync registry provenance key")?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(key),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_signing_key(&path)?.context("registry provenance key disappeared")
        }
        Err(error) => Err(error.error).context("persist registry provenance key"),
    }
}

pub fn signing_key_path(store_dir: &Path) -> PathBuf {
    store_dir.join(SIGNING_KEY_FILE)
}

fn read_signing_key(path: &Path) -> Result<Option<KeyPair>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("inspect registry provenance key"),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_SIGNING_KEY_BYTES {
        anyhow::bail!("registry provenance key is not a bounded regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            anyhow::bail!("registry provenance key permissions must be 0600");
        }
    }
    let content = std::fs::read(path).context("read registry provenance key")?;
    let key: KeyPair = serde_json::from_slice(&content).context("parse registry provenance key")?;
    let signer = key.signer().context("validate registry provenance key")?;
    let expected_key_id =
        hex::encode(&mgc_crypto::Blake3Hasher::hash_bytes(&key.public_key.0).0[..8]);
    if signer.public_key() != key.public_key || key.key_id != expected_key_id {
        anyhow::bail!("registry provenance key pair does not match");
    }
    Ok(Some(key))
}

/// Sign one canonical payload hash for the append-only per-package chain.
/// Ký hash payload đã chuẩn hóa cho chuỗi append-only của từng package.
pub fn sign_attestation(payload: AttestationPayload, key: &KeyPair) -> Result<SignedAttestation> {
    let canonical = serde_json::to_vec(&payload).context("serialize provenance payload")?;
    let entry_hash = hex::encode(sha2::Sha256::digest(canonical));
    let signer = key.signer().context("load registry provenance signer")?;
    let signature = signer.sign(entry_hash.as_bytes());
    Ok(SignedAttestation {
        payload,
        entry_hash,
        key_id: key.key_id.clone(),
        signature: base64::engine::general_purpose::STANDARD.encode(signature.0),
        sigstore_bundle: None,
    })
}

/// Create a Fulcio-backed in-toto attestation and record it in public Rekor.
/// Tạo in-toto attestation qua Fulcio và ghi vào Rekor công khai.
fn trusted_publish_predicate(
    package: &str,
    version: &str,
    registry_digest: &str,
    builder: &BuilderIdentity,
) -> serde_json::Value {
    serde_json::json!({
        "package": package,
        "version": version,
        "registryDigest": registry_digest,
        "builder": builder,
        "registryOutcome": "submission_attempted",
        "registryCommit": "not_asserted",
    })
}

pub async fn sign_sigstore_attestation(
    sigstore_oidc_token: &str,
    claims: &OidcClaims,
    package: &str,
    version: &str,
    subject_name: &str,
    registry_digest: &str,
    artifact_sha256: &str,
) -> Result<serde_json::Value> {
    use sigstore_sign::{Attestation, SigningContext, oidc::IdentityToken, types::Sha256Hash};

    let identity_token = IdentityToken::from_jwt(sigstore_oidc_token)
        .map_err(|_| anyhow::anyhow!("Sigstore identity token is malformed"))?;
    let digest =
        Sha256Hash::from_hex(artifact_sha256).context("parse artifact SHA-256 for Sigstore")?;
    let statement = Attestation::new(
        TRUSTED_PUBLISH_PREDICATE_TYPE,
        trusted_publish_predicate(
            package,
            version,
            registry_digest,
            &BuilderIdentity::from(claims),
        ),
    )
    .add_subject(subject_name, digest);
    let signer = SigningContext::production().signer(identity_token);
    let bundle = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        signer.sign_attestation(statement),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Sigstore signing exceeded the 60-second limit"))?
    .context("Sigstore Fulcio/Rekor signing failed")?;
    let value = serde_json::to_value(bundle).context("serialize Sigstore bundle")?;
    if serde_json::to_vec(&value)?.len() > MAX_SIGSTORE_BUNDLE_BYTES {
        anyhow::bail!("Sigstore bundle exceeds the configured size limit");
    }
    Ok(value)
}

/// Decode a signature using the wire format shared by registry and CLI.
/// Giải mã chữ ký theo wire format dùng chung giữa registry và CLI.
pub fn decode_signature(value: &str) -> Result<Ed25519Signature> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .context("decode provenance signature")?;
    Ok(Ed25519Signature(bytes))
}

/// Accept only npm's canonical SHA-512 integrity representation.
/// Chỉ nhận định dạng integrity SHA-512 chuẩn của npm.
pub fn is_valid_sha512_integrity(digest: &str) -> bool {
    let Some(encoded) = digest.strip_prefix("sha512-") else {
        return false;
    };
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .is_ok_and(|bytes| bytes.len() == 64)
}

/// Accept one canonical 32-byte SHA-256 hex digest.
/// Chỉ nhận chuỗi hex SHA-256 đúng 32 byte.
pub fn is_valid_sha256_hex(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "test/trusted.rs"]
mod tests;

#[derive(Deserialize)]
struct TokenExchangeRequest {
    package: String,
    #[serde(default = "default_protocol")]
    protocol: String,
    oidc_token: String,
    sigstore_oidc_token: String,
}

#[derive(Serialize)]
struct TokenExchangeResponse {
    token: String,
    expires_in: u64,
    token_type: &'static str,
}

#[derive(Deserialize)]
struct BindingRequest {
    package: String,
    #[serde(default = "default_protocol")]
    protocol: String,
    repository: String,
}

#[derive(Deserialize)]
struct SignatureQuery {
    package: String,
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    after_sequence: u64,
    #[serde(default)]
    through_sequence: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/-/v1/trusted-publish/token", post(exchange_token))
        .route("/-/v1/trusted-publish/bindings", put(set_binding))
        .route("/-/v1/trusted-publish/signatures", get(get_signatures))
        .layer(DefaultBodyLimit::max(MAX_TRUSTED_REQUEST_BYTES))
}

async fn exchange_token(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(request): Json<TokenExchangeRequest>,
) -> Result<(axum::http::HeaderMap, Json<TokenExchangeResponse>), StatusCode> {
    if !auth.trusted_enabled() {
        return Err(StatusCode::NOT_FOUND);
    }
    if !auth.trusted_transport_allowed(&headers, peer.map(|ConnectInfo(address)| address)) {
        return Err(StatusCode::FORBIDDEN);
    }
    let package = scoped_package(&request.protocol, &request.package)?;
    let (token, expires_in, claims) = auth
        .exchange_trusted_token(&package, &request.oidc_token, &request.sigstore_oidc_token)
        .await?;
    if store
        .audit(
            "trusted-publish-token-issued",
            &package,
            None,
            Some(&claims.sub),
        )
        .await
        .is_err()
    {
        auth.discard_trusted_token(&token);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok((
        headers,
        Json(TokenExchangeResponse {
            token,
            expires_in,
            token_type: "Bearer",
        }),
    ))
}

async fn set_binding(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(request): Json<BindingRequest>,
) -> Result<StatusCode, StatusCode> {
    if !auth.trusted_enabled() {
        return Err(StatusCode::NOT_FOUND);
    }
    if !auth.trusted_transport_allowed(&headers, peer.map(|ConnectInfo(address)| address)) {
        return Err(StatusCode::FORBIDDEN);
    }
    auth.authorize_admin(&headers)?;
    let package = scoped_package(&request.protocol, &request.package)?;
    validate_repository(&request.repository)?;
    store
        .set_trusted_publisher(&package, &request.repository)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_signatures(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Query(query): Query<SignatureQuery>,
) -> Result<Json<AttestationBundle>, StatusCode> {
    if !auth.trusted_enabled() {
        return Err(StatusCode::NOT_FOUND);
    }
    let package = scoped_package(&query.protocol, &query.package)?;
    if auth.admin_token.is_some() {
        if !auth.trusted_transport_allowed(&headers, peer.map(|ConnectInfo(address)| address)) {
            return Err(StatusCode::FORBIDDEN);
        }
        let user = auth
            .authenticate_headers(&headers)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !auth.can_access(&user, &package) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let limit = query.limit.unwrap_or(SIGNATURE_PAGE_SIZE);
    if limit == 0 || limit > MAX_SIGNATURE_PAGE_SIZE {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (head_sequence, entries) = store
        .trusted_attestations_page(
            &package,
            query.after_sequence,
            query.through_sequence,
            limit,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let key = auth.attestation_key().ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(AttestationBundle {
        package,
        public_key: key.public_key.to_base64(),
        key_id: key.key_id.clone(),
        head_sequence,
        entries,
    }))
}

fn default_protocol() -> String {
    "npm".to_owned()
}

/// Return the PEP 503 project key used by routes, bindings, and storage.
/// Trả về khóa project theo PEP 503 dùng thống nhất ở route, binding và storage.
pub fn canonical_pypi_name(package: &str) -> Result<String, StatusCode> {
    if package.is_empty()
        || package.len() > 214
        || !package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut normalized = String::with_capacity(package.len());
    let mut previous_separator = false;
    for byte in package.bytes() {
        if b"-_.".contains(&byte) {
            if !previous_separator {
                normalized.push('-');
            }
            previous_separator = true;
        } else {
            normalized.push(byte.to_ascii_lowercase() as char);
            previous_separator = false;
        }
    }
    Ok(normalized)
}

fn validate_oci_repository(repository: &str) -> Result<(), StatusCode> {
    if repository.is_empty() || repository.len() > 255 {
        return Err(StatusCode::BAD_REQUEST);
    }
    for component in repository.split('/') {
        let bytes = component.as_bytes();
        if bytes.is_empty()
            || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit()
            || !bytes[bytes.len() - 1].is_ascii_lowercase()
                && !bytes[bytes.len() - 1].is_ascii_digit()
            || bytes.iter().any(|byte| {
                !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && !b"._-".contains(byte)
            })
            || bytes
                .windows(2)
                .any(|pair| b"._-".contains(&pair[0]) && b"._-".contains(&pair[1]))
        {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

fn validate_package(package: &str) -> Result<(), StatusCode> {
    if package.len() > 214 || package.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let valid_part = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    };
    let valid = if let Some(scoped) = package.strip_prefix('@') {
        let Some((scope, name)) = scoped.split_once('/') else {
            return Err(StatusCode::BAD_REQUEST);
        };
        !name.contains('/') && valid_part(scope) && valid_part(name)
    } else {
        !package.contains('/') && valid_part(package)
    };
    if !valid {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

fn validate_repository(repository: &str) -> Result<(), StatusCode> {
    let mut parts = repository.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    if let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next())
        && valid_part(owner)
        && valid_part(name)
    {
        return Ok(());
    }
    Err(StatusCode::BAD_REQUEST)
}
