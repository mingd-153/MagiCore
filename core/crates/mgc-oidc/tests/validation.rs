//! OIDC validation regression tests — kiểm thử hồi quy xác minh OIDC.

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use mgc_oidc::{
    OidcClaims,
    error::OidcError,
    validate::{Jwks, validate},
};

#[test]
fn rejects_non_rs256_before_key_processing() {
    let claims = OidcClaims {
        iss: "https://issuer.example".into(),
        sub: "repo:team/project:ref:main".into(),
        aud: mgc_oidc::claims::Audience::Single("registry".into()),
        exp: u64::MAX,
        iat: 1,
        repository: None,
        job_workflow_ref: None,
        workflow_ref: None,
        event_name: None,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(b"test"),
    )
    .expect("create negative fixture");
    let result = validate(
        &token,
        &["https://issuer.example"],
        "registry",
        &Jwks { keys: vec![] },
    );
    assert!(result.is_err());
}

#[test]
fn rejects_empty_audience_and_untrusted_issuer_configuration() {
    let result = validate("not-a-jwt", &[], "", &Jwks { keys: vec![] });
    assert!(result.is_err());
}

#[tokio::test]
async fn rejects_http_identity_endpoints() {
    assert!(
        mgc_oidc::validate::discover_jwks_url("http://issuer.example")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn untrusted_issuer_is_rejected_before_any_discovery_request() {
    let token =
        "eyJhbGciOiJSUzI1NiIsImtpZCI6InRlc3QifQ.eyJpc3MiOiJodHRwczovL2F0dGFja2VyLmludmFsaWQifQ.eA";
    let result =
        mgc_oidc::validate::validate_with_discovery(token, &["https://127.0.0.1:1"], "registry")
            .await;

    assert!(
        matches!(result, Err(OidcError::UntrustedIssuer(_))),
        "untrusted JWT issuer must be rejected from the allowlist before network access"
    );
}
