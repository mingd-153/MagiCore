//! OIDC errors — typed, no token material in messages.
//! (Lỗi OIDC — typed, message không chứa token.)

use thiserror::Error;

/// OIDC failure — every variant is safe to display (never embeds tokens).
/// Lỗi OIDC — mọi biến thể đều an toàn để hiển thị, không chứa token.
#[derive(Debug, Error)]
pub enum OidcError {
    /// No OIDC token available in this environment.
    /// Không tìm thấy token OIDC trong môi trường hiện tại.
    #[error("no OIDC token available (not running in a supported CI environment)")]
    NoToken,
    /// CI token endpoint rejected the request or is unreachable.
    /// Endpoint cấp token của CI từ chối request hoặc không thể truy cập.
    #[error("CI OIDC token request failed: {0}")]
    FetchFailed(String),
    /// JWT structurally invalid.
    /// JWT sai cấu trúc.
    #[error("malformed OIDC token: {0}")]
    Malformed(String),
    /// Signature or claim check failed (reason only, never the token).
    /// Kiểm tra chữ ký hoặc claim thất bại; chỉ giữ lý do, không giữ token.
    #[error("OIDC token verification failed: {0}")]
    VerificationFailed(String),
    /// Issuer not in the allowlist.
    /// Issuer không nằm trong danh sách cho phép.
    #[error("untrusted OIDC issuer: {0}")]
    UntrustedIssuer(String),
}
