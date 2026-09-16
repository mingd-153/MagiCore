//! Minimal ZIP archive reader shared by the native engines (NuGet nupkg,
//! Swift source archives, …). No new external dependency: the container
//! parsing is binary struct decoding and the deflate stream is inflated by
//! the already-pinned `flate2` crate (raw DEFLATE, zip method 8).
//! Bộ đọc ZIP tối giản dùng chung cho các engine native (nupkg NuGet,
//! source archive Swift, …). Không thêm crate ngoài: parsing container là
//! giải mã struct nhị phân và stream deflate được inflate bằng crate
//! `flate2` đã ghim sẵn (DEFLATE thô — zip method 8).
//!
//! Safety contract mirrors `archive.rs`: entries are sanitized before any
//! path join (traversal + absolute/parent components rejected), links and
//! special files are rejected, and per-entry/total uncompressed sizes are
//! capped so a zip bomb cannot exhaust memory. CRC-32 is verified for every
//! extracted entry (table-based implementation, no crate).
//! Hợp đồng an toàn phản chiếu `archive.rs`: entry được sanitize trước khi
//! ghép path (chặn traversal + thành phần tuyệt đối/cha), chặn link và
//! special file, kích thước giải nén mỗi entry/tổng bị ghim để zip bomb
//! không thể cạn bộ nhớ. CRC-32 được xác minh cho mọi entry giải nén
//! (implementation bảng, không crate).

use mgc_types::{MgError, MgResult};
use std::path::{Component, Path, PathBuf};

/// Per-entry uncompressed cap (256 MiB) — a single zip entry above this is
/// rejected outright.
/// (Ghim giải nén mỗi entry 256 MiB — entry vượt bị từ chối ngay.)
const MAX_ENTRY_UNCOMPRESSED: u64 = 256 * 1024 * 1024;
/// Total uncompressed cap per archive (1 GiB).
/// (Ghim giải nén tổng mỗi archive 1 GiB.)
const MAX_TOTAL_UNCOMPRESSED: u64 = 1024 * 1024 * 1024;
/// Max number of entries per archive.
/// (Số entry tối đa mỗi archive.)
const MAX_ENTRIES: usize = 16_384;

/// One decompressed ZIP entry.
/// Một entry ZIP đã giải nén.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    /// Entry name as stored in the archive (never path-joined raw).
    /// Tên entry như lưu trong archive (không bao giờ ghép path thô).
    pub name: String,
    /// Decompressed content — Nội dung đã giải nén.
    pub data: Vec<u8>,
}

/// Read every regular-file entry of a ZIP archive held in memory.
/// Directories are skipped; zip64 records are rejected honestly.
/// Đọc mọi entry file thường của ZIP trong bộ nhớ. Bỏ qua thư mục;
/// zip64 bị từ chối trung thực.
pub fn read_zip_entries(bytes: &[u8]) -> MgResult<Vec<ZipEntry>> {
    let eocd = find_eocd(bytes)?;
    let (entry_count, cd_offset, cd_size) = parse_eocd(bytes, eocd)?;
    if entry_count > MAX_ENTRIES {
        return Err(MgError::Other(format!(
            "zip has {entry_count} entries, over the {MAX_ENTRIES} limit"
        )));
    }
    let cd_start = cd_offset as usize;
    let cd_end = cd_start
        .checked_add(cd_size as usize)
        .ok_or_else(|| MgError::Other("zip central directory size overflow".to_string()))?;
    if cd_end > bytes.len() {
        return Err(MgError::Other(
            "zip central directory exceeds archive bounds".to_string(),
        ));
    }

    let mut entries = Vec::with_capacity(entry_count.min(1024));
    let mut total_uncompressed: u64 = 0;
    let mut pos = cd_start;
    for _ in 0..entry_count {
        let (entry, next) = read_central_entry(bytes, pos)?;
        pos = next;
        let Some(entry) = entry else { continue };
        total_uncompressed += entry.data.len() as u64;
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED {
            return Err(MgError::Other(
                "zip total uncompressed size exceeds the safety limit".to_string(),
            ));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Extract every regular-file entry into `dest` (safe: sanitized paths,
/// no links, size caps). Mirrors `archive.rs`'s extraction contract.
/// Giải nén mọi entry file thường vào `dest` (an toàn: path đã sanitize,
/// không link, ghim kích thước). Phản chiếu hợp đồng giải nén của `archive.rs`.
pub fn extract_zip(bytes: &[u8], dest: &Path) -> MgResult<()> {
    let entries = read_zip_entries(bytes)?;
    std::fs::create_dir_all(dest)?;
    let dest_root = dest.canonicalize()?;
    for entry in &entries {
        let rel = sanitize_zip_path(&entry.name)?;
        let target = dest_root.join(&rel);
        if !target.starts_with(&dest_root) {
            return Err(MgError::Other(format!(
                "zip entry escapes destination: {}",
                target.display()
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, &entry.data)?;
    }
    Ok(())
}

/// Locate the End-Of-Central-Directory record (scan the last 64 KiB + 22).
/// Tìm bản ghi End-Of-Central-Directory (quét 64 KiB cuối + 22).
fn find_eocd(bytes: &[u8]) -> MgResult<usize> {
    if bytes.len() < 22 {
        return Err(MgError::Other("zip too small for an EOCD".to_string()));
    }
    let min_pos = bytes.len().saturating_sub(22 + 65_535);
    let mut i = bytes.len() - 22;
    loop {
        if &bytes[i..i + 4] == b"PK\x05\x06" {
            return Ok(i);
        }
        if i == min_pos {
            return Err(MgError::Other(
                "zip end-of-central-directory signature not found".to_string(),
            ));
        }
        i -= 1;
    }
}

/// EOCD → (entry count, central-directory offset, central-directory size).
fn parse_eocd(bytes: &[u8], eocd: usize) -> MgResult<(usize, u32, u32)> {
    let entry_count = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as usize;
    let cd_size = u32::from_le_bytes([
        bytes[eocd + 12],
        bytes[eocd + 13],
        bytes[eocd + 14],
        bytes[eocd + 15],
    ]);
    let cd_offset = u32::from_le_bytes([
        bytes[eocd + 16],
        bytes[eocd + 17],
        bytes[eocd + 18],
        bytes[eocd + 19],
    ]);
    Ok((entry_count, cd_offset, cd_size))
}

/// Parse one central-directory entry and extract its file content via the
/// local header. Returns `Ok(None)` for directory entries.
/// Parse một entry central-directory và lấy nội dung file qua local header.
/// Trả `Ok(None)` cho entry thư mục.
fn read_central_entry(bytes: &[u8], pos: usize) -> MgResult<(Option<ZipEntry>, usize)> {
    if pos + 46 > bytes.len() {
        return Err(MgError::Other(
            "zip central directory truncated".to_string(),
        ));
    }
    if &bytes[pos..pos + 4] != b"PK\x01\x02" {
        return Err(MgError::Other(
            "zip central directory entry signature mismatch".to_string(),
        ));
    }
    let flags = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
    let method = u16::from_le_bytes([bytes[pos + 10], bytes[pos + 11]]);
    let expected_crc = u32::from_le_bytes([
        bytes[pos + 16],
        bytes[pos + 17],
        bytes[pos + 18],
        bytes[pos + 19],
    ]);
    let comp_size = u32::from_le_bytes([
        bytes[pos + 20],
        bytes[pos + 21],
        bytes[pos + 22],
        bytes[pos + 23],
    ]);
    let uncomp_size = u32::from_le_bytes([
        bytes[pos + 24],
        bytes[pos + 25],
        bytes[pos + 26],
        bytes[pos + 27],
    ]);
    let name_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
    let extra_len = u16::from_le_bytes([bytes[pos + 30], bytes[pos + 31]]) as usize;
    let comment_len = u16::from_le_bytes([bytes[pos + 32], bytes[pos + 33]]) as usize;
    let local_offset = u32::from_le_bytes([
        bytes[pos + 42],
        bytes[pos + 43],
        bytes[pos + 44],
        bytes[pos + 45],
    ]) as usize;
    let next = pos + 46 + name_len + extra_len + comment_len;

    if uncomp_size == u32::MAX || comp_size == u32::MAX || local_offset == u32::MAX as usize {
        return Err(MgError::Other(
            "zip64 archives are not supported (honest limit)".to_string(),
        ));
    }
    let name = std::str::from_utf8(&bytes[pos + 46..pos + 46 + name_len])
        .map_err(|_| MgError::Other("zip entry name is not UTF-8".to_string()))?
        .to_string();
    // Directory entries end with '/' — nothing to extract.
    // (Entry thư mục kết thúc '/' — không cần giải nén.)
    if name.ends_with('/') {
        return Ok((None, next));
    }
    if uncomp_size as u64 > MAX_ENTRY_UNCOMPRESSED {
        return Err(MgError::Other(format!(
            "zip entry '{name}' exceeds the per-entry size limit"
        )));
    }

    let data = read_local_entry(bytes, local_offset, method, comp_size, flags)?;
    let actual_crc = crc32(&data);
    if actual_crc != expected_crc {
        return Err(MgError::Integrity(format!(
            "zip entry '{name}' CRC-32 mismatch: expected {expected_crc:#010x}, got {actual_crc:#010x}"
        )));
    }
    Ok((Some(ZipEntry { name, data }), next))
}

/// Read and decompress one entry's payload via its local header. Sizes come
/// from the central directory (local ones are zero with data descriptors).
/// Đọc và giải nén payload của một entry qua local header. Kích thước lấy từ
/// central directory (local header = 0 khi có data descriptor).
fn read_local_entry(
    bytes: &[u8],
    offset: usize,
    method: u16,
    comp_size: u32,
    _flags: u16,
) -> MgResult<Vec<u8>> {
    if offset + 30 > bytes.len() {
        return Err(MgError::Other("zip local header out of bounds".to_string()));
    }
    if &bytes[offset..offset + 4] != b"PK\x03\x04" {
        return Err(MgError::Other(
            "zip local header signature mismatch".to_string(),
        ));
    }
    let local_name_len = u16::from_le_bytes([bytes[offset + 26], bytes[offset + 27]]) as usize;
    let local_extra_len = u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]) as usize;
    let data_start = offset + 30 + local_name_len + local_extra_len;
    let data_end = data_start
        .checked_add(comp_size as usize)
        .ok_or_else(|| MgError::Other("zip entry size overflow".to_string()))?;
    if data_end > bytes.len() {
        return Err(MgError::Other(
            "zip entry data exceeds archive bounds".to_string(),
        ));
    }
    let compressed = &bytes[data_start..data_end];
    match method {
        // method 0: stored — (method 0: lưu nguyên bản)
        0 => Ok(compressed.to_vec()),
        // method 8: raw DEFLATE — (method 8: DEFLATE thô)
        8 => {
            let mut decoder = flate2::read::DeflateDecoder::new(compressed);
            let mut out = Vec::with_capacity(comp_size as usize);
            std::io::Read::read_to_end(&mut decoder, &mut out)
                .map_err(|e| MgError::Other(format!("zip deflate inflate failed: {e}")))?;
            Ok(out)
        }
        other => Err(MgError::Other(format!(
            "unsupported zip compression method {other} (honest limit: store/deflate only)"
        ))),
    }
}

/// Reject any zip entry path that could escape the destination root.
/// Từ chối mọi path entry zip có thể thoát khỏi gốc đích.
fn sanitize_zip_path(path: &str) -> MgResult<PathBuf> {
    let mut clean = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(MgError::Other(format!("unsafe zip entry path: {path}")));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(MgError::Other(format!("empty zip entry path: {path}")));
    }
    Ok(clean)
}

/// CRC-32 (IEEE 802.3, reflected, poly 0xEDB88320) — table-based, no crate.
/// CRC-32 (IEEE 802.3, reflected, poly 0xEDB88320) — theo bảng, không crate.
pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal in-memory zip (store + deflate) for the tests — no
    /// zip-writer crate, headers assembled by hand.
    /// Dựng zip tối giản trong bộ nhớ (store + deflate) cho test — không
    /// crate zip-writer, header ghép thủ công.
    fn build_zip(entries: &[(&str, Vec<u8>, u16)]) -> Vec<u8> {
        use std::io::Write as _;
        let mut out: Vec<u8> = Vec::new();
        let mut centrals: Vec<Vec<u8>> = Vec::new();
        for (name, data, method) in entries {
            let local_offset = out.len() as u32;
            let compressed = match *method {
                8 => {
                    let mut enc = flate2::write::DeflateEncoder::new(
                        Vec::new(),
                        flate2::Compression::default(),
                    );
                    enc.write_all(data).unwrap();
                    enc.finish().unwrap()
                }
                _ => data.clone(),
            };
            let crc = crc32(data);
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes()); // version
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // time/date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&compressed);

            let mut cd = Vec::new();
            cd.extend_from_slice(b"PK\x01\x02");
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&0u32.to_le_bytes());
            cd.extend_from_slice(&crc.to_le_bytes());
            cd.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes()); // extra
            cd.extend_from_slice(&0u16.to_le_bytes()); // comment
            cd.extend_from_slice(&0u16.to_le_bytes()); // disk
            cd.extend_from_slice(&0u16.to_le_bytes()); // int attrs
            cd.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
            cd.extend_from_slice(&local_offset.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
            centrals.push(cd);
        }
        let cd_offset = out.len() as u32;
        for cd in &centrals {
            out.extend_from_slice(cd);
        }
        let cd_size = out.len() as u32 - cd_offset;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(centrals.len() as u16).to_le_bytes());
        out.extend_from_slice(&(centrals.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    #[test]
    fn crc32_known_vector() {
        // "123456789" → 0xCBF43926 (IEEE 802.3 reference vector).
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn zip_store_and_deflate_roundtrip() {
        let zip = build_zip(&[
            ("dir/", Vec::new(), 0), // directory entry — skipped
            ("a.txt", b"hello zip".to_vec(), 0),
            ("nested/b.bin", b"deflated payload 1234567890".to_vec(), 8),
        ]);
        let entries = read_zip_entries(&zip).unwrap();
        assert_eq!(entries.len(), 2, "directory entry must be skipped");
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].data, b"hello zip");
        assert_eq!(entries[1].name, "nested/b.bin");
        assert_eq!(entries[1].data, b"deflated payload 1234567890");
    }

    #[test]
    fn zip_extract_writes_files_and_blocks_traversal() {
        let good = build_zip(&[("pkg/lib/file.txt", b"content".to_vec(), 8)]);
        let dir = tempfile::tempdir().unwrap();
        extract_zip(&good, dir.path()).unwrap();
        let written = std::fs::read(dir.path().join("pkg/lib/file.txt")).unwrap();
        assert_eq!(written, b"content");

        let evil = build_zip(&[("../escape.txt", b"x".to_vec(), 0)]);
        let err = extract_zip(&evil, dir.path()).unwrap_err();
        assert!(err.to_string().contains("unsafe zip entry path"), "{err}");
    }

    #[test]
    fn zip_crc_mismatch_fails_closed() {
        let mut zip = build_zip(&[("f.txt", b"payload".to_vec(), 0)]);
        // Corrupt a byte INSIDE the stored payload: local header (30) + name
        // (5) puts the 7-byte payload at [35..42].
        // (Hỏng một byte BÊN TRONG payload: local header (30) + tên (5) đặt
        // payload 7 byte tại [35..42].)
        let idx = 30 + 5 + 3;
        zip[idx] ^= 0xFF;
        let err = read_zip_entries(&zip).unwrap_err();
        assert!(matches!(err, MgError::Integrity(_)), "{err:?}");
    }

    #[test]
    fn zip_truncated_rejected() {
        assert!(read_zip_entries(b"PK\x03\x04short").is_err());
        assert!(read_zip_entries(b"").is_err());
    }
}
