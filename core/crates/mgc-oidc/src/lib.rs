//! OIDC trusted publishing — CI workload identity for package publishes.
//! (Định danh workload CI cho publish package.)
//!
//! Flow mirrors npm/PyPI trusted publishers: a CI job mints an OIDC JWT from
//! its platform issuer; the registry validates issuer, audience, expiry and
//! JWKS signature; then it checks the registered publisher binding and issues
//! a short-lived publish token. Long-lived maintainer tokens never enter CI.
//! Luồng: CI lấy JWT từ issuer; registry kiểm tra claims/chữ ký, đối chiếu binding
//! publisher rồi cấp token publish ngắn hạn. Token dài hạn không đưa vào CI.

pub mod claims;
pub mod error;
pub mod fetch;
pub mod validate;

pub use claims::OidcClaims;
pub use error::OidcError;
