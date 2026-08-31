//! TLS configuration & security (12 §10)
//! (HTTPS bắt buộc, cert validation, User-Agent, token security)

use anyhow::{bail, Context, Result};
use reqwest::ClientBuilder;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified},
    ClientConfig, DigitallySignedStruct, Error as RustlsError, RootCertStore, SignatureScheme,
};
use std::sync::Arc;

/// TLS configuration options
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Bypass cert validation (MAGICORE_ALLOW_UNTRUSTED=1) - CHỈ dev/test
    pub allow_untrusted: bool,
    /// Custom CA bundle path
    pub ca_bundle: Option<String>,
    /// Client cert (mTLS)
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    /// Minimum TLS version
    pub min_version: TlsVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    V1_2,
    V1_3,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            allow_untrusted: false,
            ca_bundle: None,
            client_cert: None,
            client_key: None,
            min_version: TlsVersion::V1_2,
        }
    }
}

impl TlsConfig {
    /// Load từ environment
    pub fn from_env() -> Self {
        Self {
            allow_untrusted: std::env::var("MAGICORE_ALLOW_UNTRUSTED")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            ca_bundle: std::env::var("MAGICORE_CA_BUNDLE").ok(),
            client_cert: std::env::var("MAGICORE_CLIENT_CERT").ok(),
            client_key: std::env::var("MAGICORE_CLIENT_KEY").ok(),
            min_version: TlsVersion::V1_2,
        }
    }

    /// Build rustls::ClientConfig
    pub fn build_rustls_config(&self) -> Result<Arc<ClientConfig>> {
        let mut root_store = RootCertStore::empty();

        // Load system roots using rustls-native-certs
        for cert in rustls_native_certs::load_native_certs().context("load native certs")? {
            root_store.add(cert)?;
        }

        // Add custom CA if provided
        if let Some(ca_path) = &self.ca_bundle {
            let ca_pem = std::fs::read_to_string(ca_path)
                .map_err(|e| anyhow::anyhow!("read CA bundle: {}", e))?;
            let certs: Vec<_> = rustls_pemfile::certs(&mut ca_pem.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("parse CA certs: {}", e))?;
            for cert in certs {
                root_store
                    .add(cert.into())
                    .map_err(|e| anyhow::anyhow!("add custom CA: {}", e))?;
            }
        }

        // Build config with TLS versions
        let builder = ClientConfig::builder_with_protocol_versions(&[
            &rustls::version::TLS12,
            &rustls::version::TLS13,
        ])
        .with_root_certificates(root_store);

        // Build config with client cert (mTLS) if provided
        let config =
            if let (Some(cert_path), Some(key_path)) = (&self.client_cert, &self.client_key) {
                let certs = load_certs(cert_path)?;
                let key = load_private_key(key_path)?;
                builder
                    .with_client_auth_cert(certs, key)
                    .map_err(|e| anyhow::anyhow!("load client cert/key: {}", e))?
            } else {
                let config = builder.with_no_client_auth();
                if self.allow_untrusted {
                    ClientConfig::builder_with_protocol_versions(&[
                        &rustls::version::TLS12,
                        &rustls::version::TLS13,
                    ])
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(NoVerify::new()))
                    .with_no_client_auth()
                } else {
                    config
                }
            };

        Ok(Arc::new(config))
    }

    /// Apply vào reqwest::ClientBuilder
    pub fn apply(&self, mut builder: ClientBuilder) -> Result<ClientBuilder> {
        if self.allow_untrusted {
            builder = builder.danger_accept_invalid_certs(true);
        }

        if let Some(ca) = &self.ca_bundle {
            let ca_pem =
                std::fs::read(ca).map_err(|e| anyhow::anyhow!("read CA: {}: {}", ca, e))?;
            builder = builder.add_root_certificate(reqwest::Certificate::from_pem(&ca_pem)?);
        }

        if let (Some(cert), Some(key)) = (&self.client_cert, &self.client_key) {
            let cert_pem = std::fs::read(cert)
                .map_err(|e| anyhow::anyhow!("read client cert: {}: {}", cert, e))?;
            let key_pem = std::fs::read(key)
                .map_err(|e| anyhow::anyhow!("read client key: {}: {}", key, e))?;
            // Combine cert and key into a single PEM for reqwest::Identity::from_pem
            let combined_pem = [cert_pem.as_slice(), key_pem.as_slice()].concat();
            let identity = reqwest::Identity::from_pem(&combined_pem)
                .map_err(|e| anyhow::anyhow!("create identity: {}", e))?;
            builder = builder.identity(identity);
        }

        Ok(builder)
    }
}

/// Certificate verifier that accepts everything (DANGEROUS)
#[derive(Debug)]
struct NoVerify;

impl NoVerify {
    fn new() -> Self {
        Self
    }
}

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
        ]
    }
}

/// Load certs from PEM file
fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let certs: Vec<_> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    Ok(certs)
}

/// Load private key from PEM file
fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut keys: Vec<PrivateKeyDer> = rustls_pemfile::pkcs8_private_keys(&mut reader)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(PrivateKeyDer::Pkcs8)
        .collect();

    if keys.is_empty() {
        // Try RSA
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        keys = rustls_pemfile::rsa_private_keys(&mut reader)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(PrivateKeyDer::Pkcs1)
            .collect();
    }

    keys.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no private key found"))
}

/// Security headers & token handling (12 §10)
pub fn secure_request_builder(
    builder: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut b = builder
        .header(
            "User-Agent",
            format!("magicore/{} (os; arch)", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/json");

    if let Some(t) = token {
        // Token chỉ trong header Authorization - KHÔNG query string
        b = b.bearer_auth(t);
    }
    b
}

/// Validate URL scheme - HTTPS bắt buộc cho remote registry
pub fn validate_registry_url(url: &str, allow_http_local: bool) -> Result<()> {
    let parsed = url::Url::parse(url)?;
    if parsed.scheme() == "http" {
        if !allow_http_local {
            bail!("HTTP not allowed for remote registry — use HTTPS");
        }
        let host = parsed.host_str().unwrap_or("");
        if host != "localhost" && host != "127.0.0.1" {
            bail!("HTTP only allowed for localhost/127.0.0.1 — use HTTPS for remote");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/tls_test.rs"]
mod tests;
