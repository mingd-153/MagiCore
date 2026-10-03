//! JWT validation against JWKS — signature + issuer + audience + expiry.
//! (Validate JWT với JWKS — chữ ký + issuer + audience + hết hạn.)

use crate::fetch::{fetch_json, secure_url};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};

use crate::claims::OidcClaims;
use crate::error::OidcError;
use base64::Engine as _;

/// One JSON Web Key (RSA only — the only algorithm this verifier accepts).
/// Một khóa JSON Web Key; verifier này chỉ chấp nhận RSA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwk {
    /// Key type — must be `"RSA"`.
    /// Loại khóa — bắt buộc là `"RSA"`.
    pub kty: String,
    /// Key id — must match the JWT header `kid`.
    /// ID khóa — phải khớp `kid` trong header JWT.
    #[serde(default)]
    pub kid: Option<String>,
    /// Base64url modulus.
    /// Modulus mã hóa Base64url.
    pub n: String,
    /// Base64url exponent.
    /// Exponent mã hóa Base64url.
    pub e: String,
}

/// A JSON Web Key Set document.
/// Tài liệu tập hợp các JSON Web Key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwks {
    /// Signing keys.
    /// Các khóa dùng để ký.
    pub keys: Vec<Jwk>,
}

/// Clock-skew tolerance applied to expiry checks.
/// Khoảng dung sai lệch đồng hồ khi kiểm tra hết hạn.
pub const EXPIRY_LEEWAY_SECS: u64 = 60;
/// Maximum accepted workload JWT lifetime — tuổi thọ tối đa của JWT workload.
pub const MAX_TOKEN_LIFETIME_SECS: u64 = 15 * 60;

/// Validate `token`: RS256 only, `kid`-pinned key, issuer + audience +
/// expiry enforced. Returns the verified identity — never the token.
/// Xác minh token bằng RS256, khóa theo `kid`, issuer, audience và hạn dùng; chỉ trả identity đã xác minh.
pub fn validate(
    token: &str,
    allowed_issuers: &[&str],
    audience: &str,
    jwks: &Jwks,
) -> Result<OidcClaims, OidcError> {
    if token.trim().is_empty()
        || token.len() > MAX_JWT_BYTES
        || audience.is_empty()
        || allowed_issuers.is_empty()
    {
        return Err(OidcError::Malformed("empty token".to_string()));
    }
    let header = decode_header(token)
        .map_err(|error| OidcError::Malformed(format!("bad JWT header: {error}")))?;
    if header.alg != Algorithm::RS256 {
        return Err(OidcError::VerificationFailed(format!(
            "unsupported JWT algorithm {:?} (RS256 only)",
            header.alg
        )));
    }
    let kid = header
        .kid
        .ok_or_else(|| OidcError::VerificationFailed("JWT has no kid".to_string()))?;
    let jwk = jwks
        .keys
        .iter()
        .find(|key| key.kid.as_deref() == Some(kid.as_str()))
        .ok_or_else(|| {
            OidcError::VerificationFailed("JWT kid matches no trusted key".to_string())
        })?;
    if jwk.kty != "RSA" {
        return Err(OidcError::VerificationFailed(format!(
            "unsupported key type '{}' (RSA only)",
            jwk.kty
        )));
    }
    let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
        .map_err(|error| OidcError::VerificationFailed(format!("unusable trusted key: {error}")))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(allowed_issuers);
    validation.leeway = EXPIRY_LEEWAY_SECS;
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let data = decode::<OidcClaims>(token, &key, &validation).map_err(|error| {
        use jsonwebtoken::errors::ErrorKind;
        match error.kind() {
            ErrorKind::ExpiredSignature => {
                OidcError::VerificationFailed("OIDC token expired".to_string())
            }
            ErrorKind::InvalidAudience => {
                OidcError::VerificationFailed("OIDC audience mismatch".to_string())
            }
            ErrorKind::InvalidIssuer => {
                OidcError::VerificationFailed("OIDC issuer mismatch".to_string())
            }
            ErrorKind::InvalidSignature => {
                OidcError::VerificationFailed("OIDC signature invalid".to_string())
            }
            _ => OidcError::VerificationFailed(format!("OIDC token rejected: {error}")),
        }
    })?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OidcError::VerificationFailed("system clock precedes Unix epoch".into()))?
        .as_secs();
    if data.claims.iat == 0
        || data.claims.iat > now.saturating_add(EXPIRY_LEEWAY_SECS)
        || data.claims.exp <= data.claims.iat
        || data.claims.exp.saturating_sub(data.claims.iat) > MAX_TOKEN_LIFETIME_SECS
    {
        return Err(OidcError::VerificationFailed(
            "invalid OIDC issuance time".into(),
        ));
    }
    Ok(data.claims)
}

/// Fetch a JWKS document (bounded timeout — no hanging publishes).
/// Tải JWKS với timeout giới hạn để publish không bị treo.
pub async fn fetch_jwks(jwks_url: &str) -> Result<Jwks, OidcError> {
    serde_json::from_value(fetch_json(secure_url(jwks_url)?, None).await?)
        .map_err(|_| OidcError::FetchFailed("invalid JWKS document".into()))
}

/// OpenID discovery: `{issuer}/.well-known/openid-configuration` → `jwks_uri`.
/// Dùng OpenID discovery để lấy `jwks_uri` từ issuer.
pub async fn discover_jwks_url(issuer: &str) -> Result<String, OidcError> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let issuer_url = secure_url(issuer)?;
    let body = fetch_json(secure_url(&url)?, None).await?;
    if body.get("issuer").and_then(|value| value.as_str()) != Some(issuer) {
        return Err(OidcError::VerificationFailed(
            "discovery issuer mismatch".into(),
        ));
    }
    let jwks = body
        .get("jwks_uri")
        .and_then(|value| value.as_str())
        .ok_or_else(|| OidcError::FetchFailed("discovery has no jwks_uri".into()))?;
    let jwks_url = secure_url(jwks)?;
    if jwks_url.origin() != issuer_url.origin() {
        return Err(OidcError::VerificationFailed(
            "cross-origin JWKS discovery is not supported".into(),
        ));
    }
    Ok(jwks_url.to_string())
}

/// Cache limits — giới hạn cache và tần suất lấy khóa mới.
pub const JWKS_CACHE_TTL_SECS: u64 = 300;
pub const JWKS_REFRESH_INTERVAL_SECS: u64 = 30;
pub const MAX_CACHED_ISSUERS: usize = 32;
pub const MAX_JWT_BYTES: usize = 16 * 1024;

struct CachedKeys {
    keys: Option<Jwks>,
    fetched_at: Instant,
    attempted_at: Instant,
}

/// Refresh is serialized and throttled, including failed requests.
/// Gộp các request refresh đồng thời và giới hạn cả lần fetch thất bại.
async fn cached_jwks(issuer: &str, kid: &str) -> Result<Jwks, OidcError> {
    type IssuerCache = Mutex<HashMap<String, Arc<Mutex<Option<CachedKeys>>>>>;
    static CACHE: OnceLock<IssuerCache> = OnceLock::new();
    let slot = {
        let mut cache = CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .await;
        if !cache.contains_key(issuer) && cache.len() >= MAX_CACHED_ISSUERS {
            return Err(OidcError::VerificationFailed(
                "trusted issuer cache limit reached".into(),
            ));
        }
        cache
            .entry(issuer.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    };
    // Different issuers never share a network lock — issuer khác không chờ chung khóa mạng.
    let mut slot = slot.lock().await;
    if let Some(entry) = slot.as_ref() {
        let fresh = entry.fetched_at.elapsed() < Duration::from_secs(JWKS_CACHE_TTL_SECS);
        if fresh
            && let Some(keys) = &entry.keys
            && keys.keys.iter().any(|key| key.kid.as_deref() == Some(kid))
        {
            return Ok(keys.clone());
        }
        if entry.attempted_at.elapsed() < Duration::from_secs(JWKS_REFRESH_INTERVAL_SECS) {
            return Err(OidcError::VerificationFailed(
                "JWKS refresh temporarily throttled".into(),
            ));
        }
    }
    let now = Instant::now();
    let entry = slot.get_or_insert(CachedKeys {
        keys: None,
        fetched_at: now,
        attempted_at: now,
    });
    entry.attempted_at = now;
    let url = discover_jwks_url(issuer).await?;
    let keys = fetch_jwks(&url).await?;
    entry.keys = Some(keys.clone());
    entry.fetched_at = Instant::now();
    Ok(keys)
}

/// Validate with cached issuer discovery; only configured issuers are fetched.
/// Cache discovery/JWKS; chỉ kết nối issuer được cấu hình từ phía server.
pub async fn validate_with_discovery(
    token: &str,
    allowed_issuers: &[&str],
    audience: &str,
) -> Result<OidcClaims, OidcError> {
    if token.len() > MAX_JWT_BYTES || allowed_issuers.len() > MAX_CACHED_ISSUERS {
        return Err(OidcError::Malformed("OIDC input exceeds size limit".into()));
    }
    let header =
        decode_header(token).map_err(|_| OidcError::Malformed("invalid JWT header".into()))?;
    if header.alg != Algorithm::RS256 {
        return Err(OidcError::VerificationFailed("RS256 is required".into()));
    }
    let kid = header
        .kid
        .filter(|kid| !kid.is_empty())
        .ok_or_else(|| OidcError::Malformed("JWT has no kid".into()))?;
    let token_issuer = unverified_issuer(token)?;
    let issuer = allowed_issuers
        .iter()
        .copied()
        .find(|configured| *configured == token_issuer)
        .ok_or_else(|| {
            OidcError::UntrustedIssuer("issuer is not in the configured allowlist".into())
        })?;
    let keys = cached_jwks(issuer, &kid).await?;
    validate(token, &[issuer], audience, &keys)
}

/// Read `iss` only to select one configured JWKS endpoint; signature checks still establish trust.
/// Chỉ đọc `iss` để chọn một JWKS endpoint đã cấu hình; chữ ký vẫn là căn cứ xác thực duy nhất.
fn unverified_issuer(token: &str) -> Result<String, OidcError> {
    let payload = token
        .split('.')
        .nth(1)
        .filter(|payload| !payload.is_empty())
        .ok_or_else(|| OidcError::Malformed("JWT has no claims payload".into()))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| OidcError::Malformed("JWT claims payload is not base64url".into()))?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| OidcError::Malformed("JWT claims payload is not valid JSON".into()))?;
    claims
        .get("iss")
        .and_then(serde_json::Value::as_str)
        .filter(|issuer| !issuer.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| OidcError::Malformed("JWT has no issuer claim".into()))
}
