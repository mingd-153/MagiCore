//! Lockfile schema v2 structures
//! Cấu trúc schema lockfile v2

use serde::{Deserialize, Serialize};

/// Lockfile v2 root structure — Cấu trúc root lockfile v2
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    /// Schema version — Phiên bản schema
    pub version: String,

    /// Metadata — Metadata
    pub metadata: LockfileMetadata,

    /// Package list — Danh sách package
    #[serde(rename = "package")]
    pub packages: Vec<Package>,
}

/// Lockfile metadata — Metadata lockfile
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockfileMetadata {
    /// Generation timestamp (ISO 8601) — Timestamp tạo (ISO 8601)
    pub generated_at: String,

    /// Generator info (e.g., "mgc/1.0.0") — Thông tin generator
    pub generator: String,

    /// Lockfile self-hash (BLAKE3, excludes signature) — Hash tự thân lockfile (BLAKE3, loại trừ signature)
    pub lockfile_hash: String,

    /// Signer info (optional) — Thông tin người ký (tùy chọn)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer: Option<SignerInfo>,
}

/// Signer information — Thông tin người ký
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignerInfo {
    /// Key ID (first 8 bytes of BLAKE3(pubkey)) — Key ID (8 bytes đầu BLAKE3(pubkey))
    pub key_id: String,

    /// Ed25519 public key (base64) — Khóa công khai Ed25519 (base64)
    pub public_key: String,

    /// Signing timestamp (ISO 8601) — Timestamp ký (ISO 8601)
    pub signed_at: String,
}

/// Package entry — Entry package
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    /// Package name — Tên package
    pub name: String,

    /// Package version — Phiên bản package
    pub version: String,

    /// Resolved URL (tarball download) — URL đã resolve (download tarball)
    pub resolved: String,

    /// Integrity hash (SRI format: blake3-base64) — Hash integrity (định dạng SRI: blake3-base64)
    pub integrity: String,

    /// Direct dependencies — Dependencies trực tiếp
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Signature file structure (mgc.lock.sig) — Cấu trúc file signature
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignatureFile {
    /// Lockfile hash (blake3-base64) — Hash lockfile
    pub lockfile_hash: String,

    /// Ed25519 signature (base64) — Chữ ký Ed25519 (base64)
    pub signature: String,

    /// Key ID — Key ID
    pub key_id: String,

    /// Signing timestamp (ISO 8601) — Timestamp ký
    pub signed_at: String,
}

impl Lockfile {
    /// Create new empty lockfile — Tạo lockfile rỗng mới
    pub fn new() -> Self {
        Lockfile {
            version: "2".to_string(),
            metadata: LockfileMetadata {
                generated_at: chrono::Utc::now().to_rfc3339(),
                generator: format!("mgc/{}", env!("CARGO_PKG_VERSION")),
                lockfile_hash: String::new(), // Will be computed later — Sẽ tính sau
                signer: None,
            },
            packages: Vec::new(),
        }
    }

    /// Add package to lockfile — Thêm package vào lockfile
    pub fn add_package(&mut self, package: Package) {
        self.packages.push(package);
    }

    /// Get package by name — Lấy package theo tên
    pub fn get_package(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Check if lockfile is signed — Kiểm tra lockfile đã ký chưa
    pub fn is_signed(&self) -> bool {
        self.metadata.signer.is_some()
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

impl Package {
    /// Create new package entry — Tạo entry package mới
    pub fn new(name: String, version: String, resolved: String, integrity: String) -> Self {
        Package {
            name,
            version,
            resolved,
            integrity,
            dependencies: Vec::new(),
        }
    }

    /// Add dependency — Thêm dependency
    pub fn add_dependency(&mut self, dep: String) {
        self.dependencies.push(dep);
    }
}

impl SignatureFile {
    /// Create new signature file — Tạo file signature mới
    pub fn new(lockfile_hash: String, signature: String, key_id: String) -> Self {
        SignatureFile {
            lockfile_hash,
            signature,
            key_id,
            signed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

// Impl FromStr trait instead of custom from_str() method
impl std::str::FromStr for SignatureFile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse format:
        // lockfile_hash = "blake3-..."
        // signature = "ed25519-..."
        // key_id = "..."
        // signed_at = "..."

        let mut lockfile_hash = None;
        let mut signature = None;
        let mut key_id = None;
        let mut signed_at = None;

        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match key {
                    "lockfile_hash" => lockfile_hash = Some(value.to_string()),
                    "signature" => signature = Some(value.to_string()),
                    "key_id" => key_id = Some(value.to_string()),
                    "signed_at" => signed_at = Some(value.to_string()),
                    _ => {} // Ignore unknown fields — Bỏ qua field không biết
                }
            }
        }

        Ok(SignatureFile {
            lockfile_hash: lockfile_hash.ok_or("missing lockfile_hash")?,
            signature: signature.ok_or("missing signature")?,
            key_id: key_id.ok_or("missing key_id")?,
            signed_at: signed_at.ok_or("missing signed_at")?,
        })
    }
}

// A9 FIX (same as Week 1): Impl Display instead of inherent to_string()
impl std::fmt::Display for SignatureFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "# MagiCore Lockfile Signature v2\n\
             # DO NOT EDIT — Generated by mgc trust sign\n\
             lockfile_hash = \"{}\"\n\
             signature = \"{}\"\n\
             key_id = \"{}\"\n\
             signed_at = \"{}\"\n",
            self.lockfile_hash, self.signature, self.key_id, self.signed_at
        )
    }
}
