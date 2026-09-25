use super::*;

#[test]
fn version_validation_anchored_both_ends() {
    // Hợp lệ: semver đủ 3 số + prerelease tùy chọn
    assert!(is_valid_version("1.1.0"));
    assert!(is_valid_version("1.1.0-rc.9"));
    // Lỗi: thiếu số, hậu tố rác, chuỗi rỗng
    assert!(!is_valid_version("1.2"));
    assert!(!is_valid_version("1.2.3evil"));
    assert!(!is_valid_version(""));
    assert!(!is_valid_version("v1.2.3"));
}

#[test]
fn artifact_names_match_release_contract() {
    // Cùng quy tắc scripts/release-artifact-contract.sh (đối chiếu bằng tay
    // với output của script cho 1.1.0-rc.9).
    assert_eq!(
        artifact_name("magicore", "1.1.0-rc.9", "macos", "arm64", "tar.gz"),
        "magicore-1.1.0-rc.9-macos-arm64.tar.gz"
    );
    assert_eq!(
        artifact_name("magicore-web", "1.1.0-rc.9", "windows", "x64", "zip"),
        "magicore-web-1.1.0-rc.9-windows-x64.zip"
    );
}

#[test]
fn probe_requires_exact_version_token() {
    // Đúng version → pass; sai/thiếu → fail (không substring).
    assert!(probe_reports_version("mgc 1.1.0-rc.9\n", "1.1.0-rc.9"));
    assert!(!probe_reports_version("mgc 1.1.0-rc.90\n", "1.1.0-rc.9"));
    assert!(!probe_reports_version("mgc 1.1.0-rc.8\n", "1.1.0-rc.9"));
    assert!(!probe_reports_version("", "1.1.0-rc.9"));
    assert!(!probe_reports_version("error: boom\n", "1.1.0-rc.9"));
}

#[test]
fn manifest_signature_roundtrip() {
    use mgc_crypto::ed25519_signer::{Ed25519PublicKey, Ed25519Signer, verify_signature};
    // Ký manifest bằng key mới, verify bằng pubkey tương ứng → pass;
    // sai key/message → fail.
    let seed = [7u8; 32];
    let signer = Ed25519Signer::from_seed(&seed).unwrap();
    let pubkey = signer.public_key();
    let message = br#"{"version":"1.1.0-rc.9","artifacts":[]}"#;
    let sig = signer.sign(message);
    assert!(verify_signature(&pubkey, message, &sig).is_ok());
    assert!(verify_signature(&pubkey, b"tampered", &sig).is_err());
    let other = Ed25519Signer::from_seed(&[9u8; 32]).unwrap().public_key();
    assert!(verify_signature(&other, message, &sig).is_err());
    // Trust-root hex round-trip (định dạng --trust-root/MGC_RELEASE_TRUST_ROOTS).
    let hex_key = hex::encode(&pubkey.0);
    let decoded = hex::decode(&hex_key).unwrap();
    assert!(verify_signature(&Ed25519PublicKey(decoded), message, &sig).is_ok());
}

#[test]
fn trust_roots_merge_flag_and_env() {
    // Flag + env gộp lại, bỏ rỗng — không có gì thì vec rỗng (đường sha256).
    assert!(merge_roots(&[], None).is_empty());
    assert_eq!(
        merge_roots(&["cc".to_string()], Some("  aa, ,bb ")),
        vec!["cc".to_string(), "aa".to_string(), "bb".to_string()]
    );
}

#[cfg(unix)]
#[test]
fn stage_rejects_symlink_named_mgc() {
    // Archive chứa symlink tên mgc trỏ ra ngoài — phải từ chối (spawn
    // symlink là thực thi file đích của attacker).
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let evil_src = dir.path().join("evil.sh");
    std::fs::write(&evil_src, b"#!/bin/sh\necho pwned\n").unwrap();
    let link = dir.path().join("mgc");
    symlink(&evil_src, &link).unwrap();
    let archive_path = dir.path().join("evil.tar.gz");
    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(enc);
        // True symlink entry (append_path would follow the link and
        // store a regular file — that case is legitimately accepted).
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name(&evil_src).unwrap();
        header.set_size(0);
        header.set_cksum();
        archive.append(&header, "mgc".as_bytes()).unwrap();
        archive.into_inner().unwrap().finish().unwrap();
    }
    let _ = link;
    let result = stage_binary(&archive_path, "tar.gz");
    assert!(
        result.is_err(),
        "symlink named mgc must be rejected, not staged"
    );
}

#[test]
fn stage_accepts_regular_mgc_binary() {
    // Sanity: file regular tên mgc vẫn stage được.
    let dir = tempfile::tempdir().unwrap();
    let bin_src = dir.path().join("real-mgc");
    std::fs::write(&bin_src, b"fake-binary-bytes").unwrap();
    let archive_path = dir.path().join("good.tar.gz");
    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(enc);
        archive.append_path_with_name(&bin_src, "mgc").unwrap();
        archive.into_inner().unwrap().finish().unwrap();
    }
    let (staging, binary) = stage_binary(&archive_path, "tar.gz").unwrap();
    assert_eq!(binary.file_name().unwrap(), "mgc");
    assert!(binary.is_file() && !binary.is_symlink());
    let _ = std::fs::remove_dir_all(staging);
}

#[test]
fn provenance_mode_is_fail_closed_by_default() {
    // P0/F2: trust roots configured → Verified, no unsigned escape even
    // with the flag (combining them is refused outright).
    // Trust root đã cấu hình → Verified, cờ cũng không mở đường unsigned.
    assert!(matches!(
        resolve_provenance_mode(&["abc".to_string()], false),
        Ok(ProvenanceMode::Verified)
    ));
    assert!(resolve_provenance_mode(&["abc".to_string()], true).is_err());
    // No roots + explicit opt-in → UnsignedWarn (the single escape hatch).
    // Không root + opt-in tường minh → UnsignedWarn (lối thoát duy nhất).
    assert!(matches!(
        resolve_provenance_mode(&[], true),
        Ok(ProvenanceMode::UnsignedWarn)
    ));
    // No roots + no flag → hard error prompting a choice (never silent
    // sha256-only).
    // Không root, không cờ → lỗi cứng (không bao giờ sha256 âm thầm).
    assert!(resolve_provenance_mode(&[], false).is_err());
}

#[test]
fn streaming_hash_matches_known_digest() {
    // P1: streaming hash must equal the one-shot digest (multi-chunk
    // file exercises the read loop, not just a single pass).
    // Hash stream phải bằng digest một lượt (file nhiều chunk).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blob.bin");
    let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &bytes).unwrap();
    use sha2::Digest;
    let expect = hex::encode(sha2::Sha256::digest(&bytes));
    let (size, actual) = sha256_file_streaming(&path).unwrap();
    assert_eq!(size, 200_000);
    assert_eq!(actual, expect);
}
