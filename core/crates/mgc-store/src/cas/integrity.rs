// Integrity hashing + typed CAS address (BLAKE3, 64-char hex).
// Hash address typed chặt: chỉ hex 64 ký tự hợp lệ — mọi hash ngoài hợp đồng
// bị từ chối (fail-closed, không panic, không unwrap_or_default).
// Fields are PRIVATE (P0-1): the 64-hex invariant is enforced by the type,
// not by constructor discipline — no caller can bypass validation.
// (Field PRIVATE (P0-1): invariant 64-hex được type cưỡng chế, không phụ
// thuộc kỷ luật gọi constructor — caller không thể bypass validation.)

use blake3::Hasher;
use std::path::{Path, PathBuf};

/// BLAKE3 hex digest length (32 bytes -> 64 hex chars).
/// Độ dài digest hex BLAKE3 chuẩn (32 byte -> 64 ký tự hex).
pub const BLAKE3_HEX_LEN: usize = 64;

/// Typed BLAKE3 digest with a compile-enforced 64-lowercase-hex invariant.
/// The fields are private: construction is only possible via `from_bytes`
/// (hashes the input itself) or `from_hash_str`/`TryFrom` (validates the
/// string fail-closed). Readers use `as_hex()` / `is_executable()`.
///
/// Digest BLAKE3 typed với invariant 64-hex-thường được cưỡng chế lúc
/// compile. Field private: chỉ dựng được qua `from_bytes` (tự hash) hoặc
/// `from_hash_str`/`TryFrom` (validate fail-closed). Đọc qua `as_hex()` /
/// `is_executable()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntegrityHash {
    /// Private: guaranteed 64 lowercase hex chars — cannot be forged.
    /// (Private: đảm bảo 64 ký tự hex thường — không thể giả mạo.)
    hash: String,
    /// Private: whether the blob is an executable (affects CAS path).
    /// (Private: blob có phải executable — ảnh hưởng path CAS.)
    executable: bool,
}

impl IntegrityHash {
    /// Hash raw bytes into a typed digest (always valid by construction).
    /// Hash bytes thô thành digest typed (luôn hợp lệ theo cấu trúc).
    pub fn from_bytes(data: &[u8], executable: bool) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(data);
        Self {
            hash: hasher.finalize().to_hex().to_string(),
            executable,
        }
    }

    /// Parse a trusted/foreign hash string into a typed digest.
    /// Accepts ONLY lowercase 64-char BLAKE3 hex — anything else (short hash,
    /// Unicode, uppercase, wrong algorithm) is rejected fail-closed instead of
    /// being silently trusted.
    ///
    /// Chuyển chuỗi hash từ nguồn ngoài (lockfile/manifest/network) thành digest
    /// typed. Chỉ nhận hex 64 ký tự thường — mọi giá trị khác bị lỗi rõ ràng
    /// (fail-closed) thay vì tin mù và panic sau này.
    pub fn from_hash_str(hash_hex: &str, executable: bool) -> Result<Self, InvalidHashError> {
        validate_blake3_hex(hash_hex)?;
        Ok(Self {
            hash: hash_hex.to_ascii_lowercase(),
            executable,
        })
    }

    /// Same as `from_hash_str` but for call sites that already hold a
    /// validated digest (internal only — still validates, defense in depth).
    /// Giống `from_hash_str` — dùng nội bộ, vẫn validate (phòng vệ nhiều lớp).
    pub fn from_hash_trusted(hash_hex: &str, executable: bool) -> Result<Self, InvalidHashError> {
        Self::from_hash_str(hash_hex, executable)
    }

    /// The 64-char lowercase hex digest (read-only accessor).
    /// Digest hex 64 ký tự thường (accessor chỉ đọc).
    pub fn as_hex(&self) -> &str {
        &self.hash
    }

    /// Whether the blob is an executable (read-only accessor).
    /// Blob có phải executable (accessor chỉ đọc).
    pub fn is_executable(&self) -> bool {
        self.executable
    }

    /// CAS path for this digest under `root` (files/blake3/xx/<hash>[.exec]).
    /// The slice is safe: the private field is guaranteed 64 lowercase hex.
    ///
    /// Path CAS của digest dưới `root` — slice an toàn vì field private được
    /// type đảm bảo 64 hex thường.
    pub fn cas_path(&self, root: &Path) -> PathBuf {
        let algo_dir = root.join("files").join("blake3");
        // Safe slice: the type guarantees 64 lowercase hex chars.
        // Slice an toàn: type đã đảm bảo 64 ký tự hex thường.
        let first2 = &self.hash[..2];
        let mut path = algo_dir.join(first2).join(&self.hash);
        if self.executable {
            path.set_extension("exec");
        }
        path
    }

    /// Integrity string form: `blake3-<base64 of raw digest>`.
    /// Chuỗi integrity: `blake3-<base64 của digest thô>`.
    pub fn to_integrity_str(&self) -> String {
        use base64::Engine;
        // hex::decode cannot fail on a validated 64-hex digest (guaranteed by
        // the private-field invariant) — but fail loudly instead of silently
        // encoding an empty digest if the invariant were ever broken.
        // hex::decode không thể lỗi với digest đã validate (invariant field
        // private đảm bảo) — nhưng phải fail to thay vì encode rỗng im lặng.
        let raw = hex::decode(&self.hash).unwrap_or_else(|e| {
            unreachable!("IntegrityHash invariant violated (validated 64-hex): {e}")
        });
        format!(
            "blake3-{}",
            base64::engine::general_purpose::STANDARD.encode(&raw)
        )
    }
}

/// Fallible conversion from a foreign hash string (P0-1: trybuild-style
/// discipline — the only public ways in are `from_bytes` and this).
/// Chuyển đổi có kiểm tra từ chuỗi hash ngoài (P0-1: chỉ `from_bytes` và
/// đây là 2 lối vào public duy nhất).
impl TryFrom<&str> for IntegrityHash {
    type Error = InvalidHashError;

    fn try_from(hash_hex: &str) -> Result<Self, Self::Error> {
        Self::from_hash_str(hash_hex, false)
    }
}

/// Error for a hash string that violates the BLAKE3 hex contract.
/// Lỗi khi chuỗi hash vi phạm hợp đồng hex BLAKE3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidHashError {
    pub input: String,
}

impl InvalidHashError {
    pub fn reason(&self) -> &'static str {
        if self.input.len() != BLAKE3_HEX_LEN {
            "expected 64 hex characters"
        } else {
            "expected lowercase hex characters only"
        }
    }
}

impl std::fmt::Display for InvalidHashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid blake3 digest '{}': {}",
            self.input,
            self.reason()
        )
    }
}

impl std::error::Error for InvalidHashError {}

/// Validate a string as a lowercase 64-char BLAKE3 hex digest.
/// Kiểm tra chuỗi có phải hex thường 64 ký tự BLAKE3 không.
pub fn validate_blake3_hex(hash_hex: &str) -> Result<(), InvalidHashError> {
    if hash_hex.len() == BLAKE3_HEX_LEN
        && hash_hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(InvalidHashError {
            input: hash_hex.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TarballEntry {
    pub path: String,
    pub data: Vec<u8>,
    pub executable: bool,
}
