//! Keyring management for Ed25519 keys
//! Quản lý keyring cho khóa Ed25519

use crate::ed25519_signer::{Ed25519PublicKey, Ed25519Signer};
use crate::{CryptoError, CryptoResult};
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Key pair wrapper — Key pair wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    /// PKCS8-encoded private key — Khóa riêng encode PKCS8
    pub private_key_pkcs8: Vec<u8>,
    /// Public key — Khóa công khai
    pub public_key: Ed25519PublicKey,
    /// Key ID (fingerprint) — Key ID (fingerprint)
    pub key_id: String,
    /// Creation timestamp — Timestamp tạo
    pub created_at: u64,
}

impl KeyPair {
    /// Generate new key pair — Tạo key pair mới
    pub fn generate() -> CryptoResult<Self> {
        let rng = SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|e| CryptoError::KeyringFailed(format!("key generation failed: {:?}", e)))?;

        let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref())?;
        let public_key = signer.public_key();

        // Generate key ID from public key hash — Tạo key ID từ hash khóa công khai
        let key_id = Self::compute_key_id(&public_key);

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(KeyPair {
            private_key_pkcs8: pkcs8_bytes.as_ref().to_vec(),
            public_key,
            key_id,
            created_at,
        })
    }

    /// Compute key ID from public key (first 8 bytes of BLAKE3 hash) 
    /// Tính key ID từ khóa công khai (8 bytes đầu của BLAKE3 hash)
    fn compute_key_id(public_key: &Ed25519PublicKey) -> String {
        use crate::blake3_signer::Blake3Hasher;
        let hash = Blake3Hasher::hash_bytes(&public_key.0);
        hex::encode(&hash.0[..8])
    }

    /// Get signer from this key pair — Lấy signer từ key pair này
    pub fn signer(&self) -> CryptoResult<Ed25519Signer> {
        Ed25519Signer::from_pkcs8(&self.private_key_pkcs8)
    }
}

/// Keyring for managing multiple keys — Keyring quản lý nhiều khóa
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyring {
    /// All key pairs — Tất cả key pairs
    pub keys: Vec<KeyPair>,
    /// Default key ID — Key ID mặc định
    pub default_key_id: Option<String>,
}

impl Keyring {
    /// Create empty keyring — Tạo keyring rỗng
    pub fn new() -> Self {
        Keyring {
            keys: Vec::new(),
            default_key_id: None,
        }
    }

    /// Load keyring from file — Load keyring từ file
    pub fn load(path: &Path) -> CryptoResult<Self> {
        let content = fs::read_to_string(path)?;
        let keyring: Keyring = serde_json::from_str(&content)?;
        Ok(keyring)
    }

    /// Save keyring to file with secure permissions — Lưu keyring vào file với quyền bảo mật
    pub fn save(&self, path: &Path) -> CryptoResult<()> {
        // Create parent directory if not exists — Tạo thư mục cha nếu chưa có
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;

        // Set secure permissions (Unix only) — Đặt quyền bảo mật (chỉ Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600); // Read/write for owner only — Chỉ owner đọc/ghi
            fs::set_permissions(path, perms)?;
        }

        Ok(())
    }

    /// Add new key pair — Thêm key pair mới
    pub fn add_key(&mut self, key_pair: KeyPair) {
        // Set as default if this is the first key — Đặt làm mặc định nếu là khóa đầu tiên
        if self.keys.is_empty() {
            self.default_key_id = Some(key_pair.key_id.clone());
        }
        self.keys.push(key_pair);
    }

    /// Get default key — Lấy khóa mặc định
    pub fn default_key(&self) -> Option<&KeyPair> {
        let key_id = self.default_key_id.as_ref()?;
        self.get_key(key_id)
    }

    /// Get key by ID — Lấy khóa theo ID
    pub fn get_key(&self, key_id: &str) -> Option<&KeyPair> {
        self.keys.iter().find(|k| k.key_id == key_id)
    }

    /// Set default key — Đặt khóa mặc định
    pub fn set_default(&mut self, key_id: &str) -> CryptoResult<()> {
        if !self.keys.iter().any(|k| k.key_id == key_id) {
            return Err(CryptoError::KeyringFailed(format!(
                "key ID not found: {}",
                key_id
            )));
        }
        self.default_key_id = Some(key_id.to_string());
        Ok(())
    }

    /// Get default keyring path — Lấy đường dẫn keyring mặc định
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .expect("home directory not found")
            .join(".megagate")
            .join("keys")
            .join("keyring.json")
    }

    /// Initialize keyring with new key if not exists 
    /// Khởi tạo keyring với khóa mới nếu chưa có
    pub fn init_if_not_exists() -> CryptoResult<Self> {
        let path = Self::default_path();
        if path.exists() {
            Self::load(&path)
        } else {
            let mut keyring = Self::new();
            let key_pair = KeyPair::generate()?;
            keyring.add_key(key_pair);
            keyring.save(&path)?;
            Ok(keyring)
        }
    }
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new()
    }
}

// Hex encoding helper — Helper encode hex
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_key_pair() {
        let key_pair = KeyPair::generate().unwrap();
        assert_eq!(key_pair.public_key.0.len(), 32);
        assert!(!key_pair.key_id.is_empty());
    }

    #[test]
    fn test_key_pair_signer() {
        let key_pair = KeyPair::generate().unwrap();
        let signer = key_pair.signer().unwrap();
        let public_key = signer.public_key();
        assert_eq!(public_key, key_pair.public_key);
    }

    #[test]
    fn test_keyring_add_key() {
        let mut keyring = Keyring::new();
        let key_pair = KeyPair::generate().unwrap();
        let key_id = key_pair.key_id.clone();

        keyring.add_key(key_pair);
        assert_eq!(keyring.keys.len(), 1);
        assert_eq!(keyring.default_key_id, Some(key_id.clone()));

        let retrieved = keyring.get_key(&key_id).unwrap();
        assert_eq!(retrieved.key_id, key_id);
    }

    #[test]
    fn test_keyring_save_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keyring.json");

        let mut keyring = Keyring::new();
        let key_pair = KeyPair::generate().unwrap();
        keyring.add_key(key_pair);

        keyring.save(&path).unwrap();
        let loaded = Keyring::load(&path).unwrap();

        assert_eq!(keyring.keys.len(), loaded.keys.len());
        assert_eq!(keyring.default_key_id, loaded.default_key_id);
    }

    #[test]
    fn test_keyring_default_key() {
        let mut keyring = Keyring::new();
        let key1 = KeyPair::generate().unwrap();
        let key2 = KeyPair::generate().unwrap();

        keyring.add_key(key1.clone());
        keyring.add_key(key2.clone());

        let default = keyring.default_key().unwrap();
        assert_eq!(default.key_id, key1.key_id);

        keyring.set_default(&key2.key_id).unwrap();
        let default = keyring.default_key().unwrap();
        assert_eq!(default.key_id, key2.key_id);
    }

    #[test]
    #[cfg(unix)]
    fn test_keyring_secure_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let path = dir.path().join("keyring.json");

        let mut keyring = Keyring::new();
        let key_pair = KeyPair::generate().unwrap();
        keyring.add_key(key_pair);

        keyring.save(&path).unwrap();

        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o777, 0o600); // Owner read/write only — Chỉ owner đọc/ghi
    }
}
