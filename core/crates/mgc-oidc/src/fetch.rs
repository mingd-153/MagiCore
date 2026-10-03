//! CI OIDC token acquisition — GitHub Actions plus generic env/file input.
//! (Lấy token OIDC CI — GitHub Actions + env/file chung.)

use crate::error::OidcError;
use std::time::Duration;

/// Shared request limits — giới hạn chung cho request định danh.
pub const REQUEST_TIMEOUT_SECS: u64 = 15;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// HTTPS only, without URL credentials — chỉ HTTPS, không nhận credential trong URL.
pub(crate) fn secure_url(raw: &str) -> Result<reqwest::Url, OidcError> {
    let url = reqwest::Url::parse(raw)
        .map_err(|_| OidcError::FetchFailed("invalid identity endpoint URL".into()))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(OidcError::FetchFailed(
            "identity endpoint requires HTTPS without credentials or fragment".into(),
        ));
    }
    Ok(url)
}

/// Bound time, redirects and response size; never expose endpoint URLs in errors.
/// Giới hạn thời gian, redirect, kích thước; không đưa URL nhạy cảm vào lỗi.
pub(crate) async fn fetch_json(
    url: reqwest::Url,
    bearer: Option<&str>,
) -> Result<serde_json::Value, OidcError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|_| OidcError::FetchFailed("cannot initialize identity client".into()))?;
    let mut request = client.get(url);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| OidcError::FetchFailed("identity request failed".into()))?;
    if !response.status().is_success() {
        return Err(OidcError::FetchFailed(format!(
            "identity endpoint returned {}",
            response.status()
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| OidcError::FetchFailed("identity response read failed".into()))?
    {
        if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
            return Err(OidcError::FetchFailed(
                "identity response exceeds size limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body)
        .map_err(|_| OidcError::FetchFailed("invalid identity JSON response".into()))
}

/// Env var carrying a pre-minted OIDC JWT (generic CI, tests, escape hatch).
/// Biến môi trường chứa JWT OIDC đã cấp sẵn cho CI chung hoặc kiểm thử.
pub const OIDC_TOKEN_ENV: &str = "MGC_OIDC_TOKEN";
/// File carrying a pre-minted OIDC JWT (some CI systems mount it).
/// File chứa JWT OIDC đã cấp sẵn trên các CI mount token vào file.
pub const OIDC_TOKEN_FILE_ENV: &str = "MGC_OIDC_TOKEN_FILE";
/// Explicit OIDC token with the public Sigstore audience.
/// Token OIDC riêng có audience Sigstore công khai.
pub const SIGSTORE_OIDC_TOKEN_ENV: &str = "MGC_SIGSTORE_OIDC_TOKEN";
/// File carrying an OIDC token with the public Sigstore audience.
/// File chứa OIDC token riêng có audience Sigstore công khai.
pub const SIGSTORE_OIDC_TOKEN_FILE_ENV: &str = "MGC_SIGSTORE_OIDC_TOKEN_FILE";

/// Fetch an OIDC JWT for `audience`: explicit env/file input wins (generic
/// CI), otherwise GitHub Actions is attempted. Never logs the token.
/// Lấy JWT cho audience: ưu tiên env/file, nếu không có thì thử GitHub Actions; không ghi token vào log.
pub async fn fetch_token(audience: &str) -> Result<String, OidcError> {
    if let Ok(token) = std::env::var(OIDC_TOKEN_ENV) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    if let Ok(path) = std::env::var(OIDC_TOKEN_FILE_ENV) {
        let token = std::fs::read_to_string(&path).map_err(|error| {
            OidcError::FetchFailed(format!("cannot read OIDC token file: {error}"))
        })?;
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    fetch_github_token(audience).await
}

/// Fetch a separate token for Fulcio; a registry-audience token is not reusable.
/// Lấy token riêng cho Fulcio; token audience registry không thể dùng thay thế.
pub async fn fetch_sigstore_token() -> Result<String, OidcError> {
    if let Ok(token) = std::env::var(SIGSTORE_OIDC_TOKEN_ENV) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    if let Ok(path) = std::env::var(SIGSTORE_OIDC_TOKEN_FILE_ENV) {
        let token = std::fs::read_to_string(path).map_err(|error| {
            OidcError::FetchFailed(format!("cannot read Sigstore OIDC token file: {error}"))
        })?;
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    fetch_github_token("sigstore").await
}

/// GitHub Actions flow: `GET $ACTIONS_ID_TOKEN_REQUEST_URL&audience=…`
/// with `Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN`.
/// Response shape: `{"value": "<jwt>"}`.
/// Gọi endpoint GitHub Actions bằng bearer token và đọc JWT từ trường `value`.
pub async fn fetch_github_token(audience: &str) -> Result<String, OidcError> {
    let base = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").map_err(|_| OidcError::NoToken)?;
    let bearer = std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").map_err(|_| OidcError::NoToken)?;
    if base.trim().is_empty() || bearer.trim().is_empty() {
        return Err(OidcError::NoToken);
    }
    let mut url = secure_url(&base)?;
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "audience")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.query_pairs_mut()
        .clear()
        .extend_pairs(pairs)
        .append_pair("audience", audience);
    let body = fetch_json(url, Some(bearer.trim())).await?;
    body.get("value")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OidcError::FetchFailed("token response has no usable value".to_string()))
}
