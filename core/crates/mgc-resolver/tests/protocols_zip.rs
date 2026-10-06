//! Shared minimal ZIP reader tests (used by NuGet/Swift/Go engines).
//! Test bộ đọc ZIP dùng chung (NuGet/Swift/Go engine dùng).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::zip_reader::{crc32, read_zip_entries};

#[test]
fn crc32_reference_vectors() {
    // IEEE 802.3 reference vector + empty input.
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn zip_reader_rejects_truncated_input() {
    assert!(read_zip_entries(b"").is_err());
    assert!(read_zip_entries(b"PK\x03\x04orphan").is_err());
}
