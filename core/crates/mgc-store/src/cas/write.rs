// CAS write primitives: hash-while-writing, atomic unique-temp + fsync.
// Nguyên thủy ghi CAS: hash trong lúc ghi, temp duy nhất + fsync + rename
// nguyên tử (không ghi đè file đang dùng, không lộ blob nửa vời).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::integrity::IntegrityHash;
use super::store::StoreError;

/// Files larger than this threshold will be streamed through a buffered reader
/// instead of being fully loaded into memory.
/// File lớn hơn ngưỡng này được stream qua buffered reader thay vì load hết
/// vào memory.
pub const STREAM_THRESHOLD: usize = 1024 * 1024; // 1 MB

/// Write data to file, fsync, and set permissions. The caller writes to a
/// unique temp path and renames into place — the fsync makes the rename
/// durable so a crash cannot publish a torn CAS blob.
///
/// Ghi data vào file, fsync rồi đặt permission. Caller ghi vào temp duy nhất
/// và rename vào vị trí cuối — fsync giúp rename bền vững, crash không thể
/// công bố blob CAS nửa vời.
pub fn write_all_verify_and_set_perms(
    mut writer: fs::File,
    dest: &Path,
    data: &[u8],
    executable: bool,
) -> Result<IntegrityHash, StoreError> {
    let expected = IntegrityHash::from_bytes(data, executable);

    writer.write_all(data).map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: format!("write failed: {e}"),
    })?;
    writer
        .flush()
        .and_then(|_| writer.sync_all())
        .map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: format!("fsync failed: {e}"),
        })?;

    set_permissions(dest, executable)?;

    Ok(expected)
}

/// Stream data from reader to writer while computing the content hash.
/// Durability (fsync) is the caller's contract via `finalize_streamed_write`.
/// Stream data từ reader sang writer trong lúc tính hash. Độ bền (fsync) do
/// caller đảm nhận qua `finalize_streamed_write`.
pub fn stream_write_verify_and_set_perms(
    mut writer: fs::File,
    dest: &Path,
    mut reader: impl Read,
    executable: bool,
) -> Result<IntegrityHash, StoreError> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = reader.read(&mut buf).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: format!("read failed: {e}"),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n]).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: format!("write failed: {e}"),
        })?;
    }

    let hash = hasher.finalize().to_hex().to_string();
    // Private-field invariant (P0-1): construct through the validating path.
    // (Invariant field private (P0-1): dựng qua đường có validate.)
    let integrity = IntegrityHash::from_hash_trusted(&hash, executable)
        .map_err(|e| StoreError::InvalidHash(e.input))?;

    writer
        .flush()
        .and_then(|_| writer.sync_all())
        .map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: format!("fsync failed: {e}"),
        })?;
    set_permissions(dest, executable)?;

    Ok(integrity)
}

/// Unique temp path for atomic CAS writes — never a fixed name (concurrent
/// writers must not collide or clobber each other's temp files).
/// Đường dẫn temp duy nhất cho ghi CAS nguyên tử — không bao giờ tên cố định
/// (nhiều writer song song không đụng độ hay đè temp của nhau).
pub fn unique_tmp_path(dir: &Path, prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tid = {
        let tid = std::thread::current().id();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&tid, &mut hasher);
        std::hash::Hasher::finish(&hasher)
    };
    dir.join(format!(
        "{prefix}-{}-{tid:x}-{nanos}-{count}",
        std::process::id()
    ))
}

// ── TempFileName parser (Gate 11-B / P0-7, vòng-11 audit) ─────────────
//
// The doctor's orphan-temp GC used a LOOSE heuristic (`has '-', ≥4 dash
// parts, last part all digits`) which matched foreign names like
// `customer-important-backup-123` and deleted them once stale. The
// parser below is the SINGLE shared authority: a name is MGC-owned iff
// its PREFIX is in the allowlist AND the remaining fields parse exactly
// as `<pid>-<tid_hex>-<nanos>-<counter>`. The doctor calls this instead
// of guessing by shape.
//
// (Parser TempFileName (P0-7): GC temp mồ côi của doctor từng dùng
// heuristic LỎNG (`có '-', ≥4 phần, phần cuối toàn số`) — khớp cả tên
// ngoài như `customer-important-backup-123` và xóa nó khi đủ cũ. Parser
// dưới đây là nguồn duy nhất: một tên là CỦA MGC khi PREFIX nằm trong
// allowlist VÀ các field còn lại parse ĐÚNG dạng
// `<pid>-<tid_hex>-<nanos>-<counter>`. Doctor gọi cái này thay vì đoán
// theo hình dạng.)

/// Where an MGC temp file may legally live — a name alone is not enough,
/// the doctor must also confirm the DIRECTORY matches the purpose.
/// (Nơi một temp file MGC được phép nằm — tên thôi chưa đủ, doctor phải
/// xác nhận cả THƯ MỤC khớp purpose.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempPurpose {
    /// CAS write/import staging — lives under `<store>/cas/tmp/`.
    /// (Staging ghi/import CAS — nằm trong `<store>/cas/tmp/`.)
    CasTmp,
    /// Compiled-record atomic write — lives next to the record (records/).
    /// (Ghi nguyên tử record biên dịch — nằm cạnh record (records/).)
    CompiledRecord,
}

/// The exact prefixes `unique_tmp_path` is called with today. `mgc-export`
/// and `mgc-export-probe` stage INSIDE the DESTINATION directory (never
/// the CAS tmp dir) and are therefore NEVER swept by the CAS tmp GC —
/// they are listed here only so a stray one found in tmp/ is recognized
/// as misfiled MGC data, not foreign data.
/// (Các prefix mà `unique_tmp_path` được gọi hôm nay. `mgc-export` và
/// `mgc-export-probe` staging TRONG THƯ MỤC ĐÍCH (không bao giờ tmp của
/// CAS) nên KHÔNG BAO GIỜ bị GC tmp CAS quét — liệt kê ở đây chỉ để một
/// file lạc trong tmp/ được nhận ra là dữ liệu MGC bị đặt sai chỗ, không
/// phải dữ liệu ngoài.)
const MGC_TEMP_PREFIXES: &[&str] = &[
    "import-file",
    "write-bytes",
    "compiled",
    "mgc-export",
    "mgc-export-probe",
];

/// Prefixes whose temps legally live under the CAS tmp directory — the
/// ONLY set the doctor's CAS-tmp GC may consider for sweeping.
/// (Prefix mà temp hợp pháp nằm trong thư mục tmp của CAS — tập DUY
/// NHẤT mà GC tmp của doctor được xét để quét.)
const CAS_TMP_SWEEPABLE_PREFIXES: &[&str] = &["import-file", "write-bytes"];

/// A parsed MGC temp file name — proof the name matches the exact
/// `<prefix>-<pid>-<tid_hex>-<nanos>-<counter>` schema.
/// (Tên temp MGC đã parse — bằng chứng tên khớp ĐÚNG schema
/// `<prefix>-<pid>-<tid_hex>-<nanos>-<counter>`.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempFileName {
    pub prefix: String,
    pub pid: u32,
    pub tid_hex: String,
    pub nanos: u128,
    pub counter: u64,
}

impl TempFileName {
    /// Strict parser — a name is MGC-owned ONLY if the prefix is in the
    /// allowlist and every remaining field parses with the exact type
    /// and arity. `customer-important-backup-123` fails (prefix not in
    /// the list); `write-bytes-1234-ab-999-0` parses; a truncated or
    /// extra-field name fails. NOTE: tid is parsed as non-empty HEX.
    /// (Parser nghiêm ngặt — tên là CỦA MGC CHỈ KHI prefix nằm trong
    /// allowlist và mọi field còn lại parse đúng type và đúng số lượng.
    /// `customer-important-backup-123` fail (prefix ngoài list);
    /// `write-bytes-1234-ab-999-0` parse; tên thiếu/thừa field fail.
    /// LƯU Ý: tid parse dạng hex không rỗng.)
    pub fn parse(name: &str) -> Option<Self> {
        // Prefixes themselves contain dashes (`import-file`, `mgc-export`)
        // — split on '-' would shred them. Match the LONGEST allowlisted
        // prefix first (both export prefixes), then split the REMAINDER
        // into exactly 4 fields: pid, tid_hex, nanos, counter.
        // (Chính prefix chứa dấu gạch (`import-file`, `mgc-export`) —
        // split theo '-' sẽ xé chúng. Match prefix dài nhất trong
        // allowlist trước (cả 2 prefix export), rồi split PHẦN CÒN LẠI
        // đúng 4 field: pid, tid_hex, nanos, counter.)
        let prefix = MGC_TEMP_PREFIXES
            .iter()
            .filter(|p| name.starts_with(&format!("{p}-")))
            .max_by_key(|p| p.len())?;
        let rest = &name[prefix.len() + 1..];
        // Split keeping empty fields so `a--b` is rejected, not silently
        // field-skipped.
        // (Split giữ field rỗng để `a--b` bị từ chối, không lướt qua.)
        let parts: Vec<&str> = rest.split('-').collect();
        if parts.len() != 4 {
            return None;
        }
        let pid: u32 = parts[0].parse().ok()?;
        let tid_hex = parts[1];
        if tid_hex.is_empty() || !tid_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let nanos: u128 = parts[2].parse().ok()?;
        let counter: u64 = parts[3].parse().ok()?;
        Some(Self {
            prefix: prefix.to_string(),
            pid,
            tid_hex: tid_hex.to_string(),
            nanos,
            counter,
        })
    }

    /// May the CAS-tmp GC sweep this parsed name? Only temps that
    /// legitimately stage under CAS tmp/ (`import-file`, `write-bytes`)
    /// — and nothing else, even other MGC-prefixed names.
    /// (GC tmp CAS có được quét tên đã parse này? Chỉ temp hợp pháp
    /// staging dưới tmp CAS (`import-file`, `write-bytes`) — không gì
    /// khác, kể cả tên có prefix MGC khác.)
    pub fn cas_tmp_sweepable(&self) -> bool {
        CAS_TMP_SWEEPABLE_PREFIXES.contains(&self.prefix.as_str())
    }
}

// Tests live in tests/ (RULE §5) — see tests/temp_name.rs.
// (Test nằm trong tests/ (RULE §5) — xem tests/temp_name.rs.)

/// Sync the parent directory of a just-renamed file so the rename itself is
/// crash-durable (audit vòng-3 P1-7: fsync on the FILE without a directory
/// fsync leaves the rename un-durable across power loss). Unix only; on
/// other platforms this is a documented no-op (Windows NTFS journals
/// metadata differently).
///
/// Đồng bộ thư mục cha của file vừa rename để chính rename được bền vững
/// khi crash (P1-7 audit vòng-3: fsync FILE mà không fsync thư mục thì
/// rename chưa bền qua mất điện). Chỉ Unix; platform khác là no-op được
/// ghi rõ (Windows NTFS journal metadata theo cách khác).
pub fn sync_parent_dir(dir: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| StoreError::Io {
                path: dir.to_path_buf(),
                msg: format!("fsync parent directory failed: {e}"),
            })?;
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
    Ok(())
}

fn set_permissions(path: &Path, executable: bool) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // CAS-internal blobs are owner-only (P0-D, Tech Lead vòng-7
        // 2026-09-13): the store holds package bytes, compiled JS and inline
        // sourcemaps — world/group-readable modes leak them to other local
        // users. The executable BIT is still tracked (0700 vs 0600) so the
        // export path can restore the right mode on the project side
        // (set_exported_permissions applies 0755/0644 there).
        // Blob bên trong CAS chỉ-owner (P0-D): store giữ bytes package, JS
        // đã biên dịch và sourcemap inline — mode cho group/others đọc được
        // sẽ lộ chúng cho user local khác. BIT executable vẫn được theo dõi
        // (0700 với 0600) để đường export khôi phục đúng mode phía project
        // (set_exported_permissions áp 0755/0644 ở đó).
        let mode = if executable { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| {
            StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("set permissions failed: {e}"),
            }
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
    }
    Ok(())
}
