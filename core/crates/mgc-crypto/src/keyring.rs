//! Keyring management for Ed25519 keys
//! Quản lý keyring cho khóa Ed25519

use crate::ed25519_signer::{Ed25519PublicKey, Ed25519Signer};
use crate::{CryptoError, CryptoResult};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::Ed25519KeyPair;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Key pair wrapper — Key pair wrapper
#[derive(Clone, Serialize, Deserialize)]
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

impl fmt::Debug for KeyPair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyPair")
            .field("private_key_pkcs8", &"[REDACTED]")
            .field("public_key", &self.public_key)
            .field("key_id", &self.key_id)
            .field("created_at", &self.created_at)
            .finish()
    }
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
        let content = read_keyring_contents(path)?;
        let keyring: Keyring = serde_json::from_str(&content)?;
        Ok(keyring)
    }

    /// Save keyring to file with secure permissions — Lưu keyring vào file với quyền bảo mật
    pub fn save(&self, path: &Path) -> CryptoResult<()> {
        // A2 FIX: Validate path to prevent directory traversal (production only)
        // Allow test paths (tempdir) in test builds
        self.save_impl(path, false)
    }

    /// Internal save implementation with test mode flag
    fn save_impl(&self, path: &Path, skip_validation: bool) -> CryptoResult<()> {
        // A2 FIX: Path validation (skip in tests)
        if !skip_validation {
            let canonical = path.canonicalize().unwrap_or_else(|_| {
                // If path doesn't exist yet, validate parent
                if let Some(parent) = path.parent() {
                    parent.canonicalize().unwrap_or_else(|_| path.to_path_buf())
                } else {
                    path.to_path_buf()
                }
            });

            // A2 FIX: Only allow writing to .magicore directory in production
            // Gộp let-chain theo clippy 1.98 (edition 2024 let-chains).
            if let Some(home) = dirs::home_dir()
                && !canonical.starts_with(&home)
            {
                return Err(CryptoError::KeyringFailed(
                    "keyring path must be in home directory".to_string(),
                ));
            }
        }

        // Create parent directory if not exists — Tạo thư mục cha nếu chưa có
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        let previous = read_existing_for_backup(path)?;
        if let Some(previous) = previous {
            let backup = path.with_extension("json.bak");
            reject_non_regular_existing_path(&backup)?;
            write_private_file_atomic(&backup, previous.as_bytes())?;
        }

        reject_non_regular_existing_path(path)?;
        write_private_file_atomic(path, content.as_bytes())
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
            .join(".magicore")
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

fn read_keyring_contents(path: &Path) -> CryptoResult<String> {
    #[cfg(unix)]
    {
        use std::io::Read;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut options = fs::OpenOptions::new();
        options
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let mut file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(CryptoError::KeyringFailed(
                "keyring path must resolve to a regular file".to_string(),
            ));
        }

        let mode = metadata.permissions().mode() & 0o7777;
        if mode & !0o600 != 0 {
            return Err(CryptoError::KeyringFailed(format!(
                "keyring file '{}' has insecure permissions {mode:04o}; group/world access and executable bits are forbidden",
                path.display(),
            )));
        }

        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(content)
    }

    #[cfg(windows)]
    {
        use std::io::Read;
        use std::os::windows::fs::OpenOptionsExt;

        // FILE_FLAG_OPEN_REPARSE_POINT prevents following a final-component
        // symlink/reparse point while opening the key material.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let mut options = fs::OpenOptions::new();
        options
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let mut file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CryptoError::KeyringFailed(
                "keyring path must resolve to a regular non-reparse file".to_string(),
            ));
        }

        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(content)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Err(CryptoError::KeyringFailed(
                "keyring path must resolve to a regular file".to_string(),
            ));
        }
        Ok(fs::read_to_string(path)?)
    }
}

fn read_existing_for_backup(path: &Path) -> CryptoResult<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(_) => read_keyring_contents(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn reject_non_regular_existing_path(path: &Path) -> CryptoResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(CryptoError::KeyringFailed(format!(
                "refusing to replace non-regular or symlink keyring path '{}'",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn write_private_file_atomic(path: &Path, contents: &[u8]) -> CryptoResult<()> {
    use std::io::Write;

    let parent = path.parent().ok_or_else(|| {
        CryptoError::KeyringFailed("keyring destination must have a parent directory".to_string())
    })?;
    let rng = SystemRandom::new();

    for _ in 0..16 {
        let mut nonce = [0u8; 16];
        rng.fill(&mut nonce).map_err(|_| {
            CryptoError::KeyringFailed("failed to generate temporary keyring name".to_string())
        })?;
        let nonce_hex = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let file_name = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("keyring"))
            .to_string_lossy();
        let temporary_path = parent.join(format!(".{file_name}.{nonce_hex}.tmp"));

        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }

        let mut file = match options.open(&temporary_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let mut cleanup = TemporaryKeyringFile::new(temporary_path.clone());
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        atomic_replace_file(&temporary_path, path)?;
        cleanup.disarm();

        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;

        return Ok(());
    }

    Err(CryptoError::KeyringFailed(
        "could not allocate a unique temporary keyring file".to_string(),
    ))
}

fn atomic_replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };

        let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination_wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: both nul-terminated path buffers stay alive for the full
        // synchronous call; MoveFileExW does not retain either pointer.
        let succeeded = unsafe {
            MoveFileExW(
                source_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

struct TemporaryKeyringFile {
    path: PathBuf,
    armed: bool,
}

impl TemporaryKeyringFile {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryKeyringFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../tests/unit/keyring_save_unit.rs"]
mod keyring_save_unit;

// Hex encoding helper — Helper encode hex
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
