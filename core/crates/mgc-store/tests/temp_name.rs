#![allow(clippy::unwrap_used)]
//! Contract tests for the shared TempFileName parser (Gate 11-B / P0-7,
//! vòng-11 audit): the doctor's orphan-temp GC used to guess MGC temps by
//! SHAPE (`has '-', ≥4 parts, digits tail`) and deleted foreign dashed
//! names like `customer-important-backup-123`. The parser is now the
//! single authority: exact prefix allowlist + exact
//! `<prefix>-<pid>-<tid_hex>-<nanos>-<counter>` arity.
//! test riêng tại test/ (RULE §5).
//! (Test hợp đồng cho parser TempFileName dùng chung: GC temp mồ côi của
//! doctor từng đoán temp MGC theo HÌNH DẠNG rồi xóa tên ngoài như
//! `customer-important-backup-123`. Parser giờ là nguồn duy nhất: allowlist
//! prefix chính xác + arity `<prefix>-<pid>-<tid_hex>-<nanos>-<counter>`.)

use mgc_store::cas::TempFileName;
use mgc_store::cas::write::unique_tmp_path;

#[test]
fn real_unique_tmp_paths_parse_roundtrip() {
    // Names produced by the ACTUAL write primitive must parse — the
    // parser and the generator cannot drift apart.
    // (Tên do primitive ghi THẬT sinh phải parse được — parser và bộ sinh
    // không được lệch nhau.)
    for prefix in ["import-file", "write-bytes", "compiled", "mgc-export"] {
        let path = unique_tmp_path(std::path::Path::new("/tmp/x"), prefix);
        let name = path.file_name().unwrap().to_str().unwrap();
        let parsed = TempFileName::parse(name)
            .unwrap_or_else(|| panic!("generated temp name must parse: {name}"));
        assert_eq!(parsed.prefix, prefix);
        // Only CAS-tmp-legal prefixes are sweepable in the CAS tmp dir.
        // (Chỉ prefix hợp pháp dưới tmp CAS mới được quét trong tmp CAS.)
        let sweepable = parsed.cas_tmp_sweepable();
        assert_eq!(
            sweepable,
            matches!(prefix, "import-file" | "write-bytes"),
            "sweepable must be exact for {prefix}"
        );
    }
}

#[test]
fn foreign_dashed_names_never_parse() {
    // The exact names from the audit that the old heuristic deleted.
    // (Đúng các tên từ audit mà heuristic cũ từng xóa.)
    for foreign in [
        "customer-important-backup-123",
        "project-data-copy-2026",
        "important-backup-1-2-3",
        "write-bytes-",
        "write-bytes-x-99-1",
        "write-bytes-12-ab-99",
        "write-bytes-12-ab-99-1-2",
        "-write-bytes-12-ab-99-1",
        "write--bytes-12-ab-99-1",
    ] {
        assert!(
            TempFileName::parse(foreign).is_none(),
            "foreign name must NEVER parse as MGC temp: {foreign}"
        );
    }
}

#[test]
fn mgc_prefixed_but_misfiled_names_are_not_cas_tmp_sweepable() {
    // `mgc-export` / `mgc-export-probe` temps legally stage in the
    // DESTINATION directory, never the CAS tmp dir — even when they
    // parse, the CAS-tmp GC must not sweep them.
    // (Temp `mgc-export` / `mgc-export-probe` hợp pháp staging trong
    // thư mục ĐÍCH, không bao giờ trong tmp CAS — kể cả khi parse, GC
    // tmp CAS cũng không được quét.)
    let path = unique_tmp_path(std::path::Path::new("/tmp/x"), "mgc-export");
    let name = path.file_name().unwrap().to_str().unwrap();
    let parsed = TempFileName::parse(name).expect("export temp parses");
    assert!(
        !parsed.cas_tmp_sweepable(),
        "mgc-export must never be swept by the CAS-tmp GC"
    );
    let path = unique_tmp_path(std::path::Path::new("/tmp/x"), "mgc-export-probe");
    let name = path.file_name().unwrap().to_str().unwrap();
    let parsed = TempFileName::parse(name).expect("probe temp parses");
    assert!(
        !parsed.cas_tmp_sweepable(),
        "mgc-export-probe must never be swept by the CAS-tmp GC"
    );
}

#[test]
fn strict_field_types() {
    // tid must be HEX, pid/nanos/counter must be digits — a decimal tid
    // (like a thread id from another runtime) must not parse.
    // (tid phải HEX, pid/nanos/counter phải toàn số — tid thập phân
    // (như thread id của runtime khác) không được parse.)
    assert!(TempFileName::parse("import-file-42-1a2b-99-0").is_some());
    assert!(TempFileName::parse("import-file-42-1a2b-99-x").is_none());
    assert!(TempFileName::parse("import-file-x-1a2b-99-0").is_none());
    assert!(TempFileName::parse("import-file-42-1a2b-x-0").is_none());
}
