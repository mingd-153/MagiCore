#![allow(clippy::unwrap_used)]
//! Integration tests for tarball builder — test riêng tại test/ (RULE §5)
use mg_pack::tarball::pack;
use std::fs;
use std::io::Write;

fn write(root: &std::path::Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = fs::File::create(p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn pack_produces_deterministic_tarball() {
    let src_tmp = tempfile::tempdir().unwrap();
    let root = src_tmp.path();
    write(root, "package.json", r#"{"name":"x","version":"1.0.0"}"#);
    write(root, "src/index.js", "export {}");

    // output OUTSIDE root — tránh self-pack (file tgz bị quét lại)
    let out_tmp = tempfile::tempdir().unwrap();
    let out1 = out_tmp.path().join("a.tgz");
    let out2 = out_tmp.path().join("b.tgz");
    let r1 = pack(root, &out1, "x-1.0.0").unwrap();
    let r2 = pack(root, &out2, "x-1.0.0").unwrap();

    let d1 = fs::read(&out1).unwrap();
    let d2 = fs::read(&out2).unwrap();
    assert_eq!(d1, d2, "same input must produce byte-identical tarballs");

    assert_eq!(r1.shasum, r2.shasum);
    assert_eq!(r1.integrity, r2.integrity);
    assert!(r1.shasum.len() == 40, "sha1 hex must be 40 characters");
    assert!(r1.integrity.starts_with("sha512-"));
    assert!(r1.size > 0);
    assert_eq!(r1.unpacked_size, 39); // package.json (30) + src/index.js (9) = 39
    assert_eq!(r1.entry_count, 2);
}

#[test]
fn pack_uses_prefix_in_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "package.json", "{}");

    let out = tmp.path().join("p.tgz");
    pack(root, &out, "x-1.0.0").unwrap();
    let data = fs::read(&out).unwrap();
    let tarball = fs::File::open(&out).unwrap();
    let mut gz = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(&mut gz);
    let names: Vec<String> = archive
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"x-1.0.0/package.json".to_string()));
    assert!(!data.is_empty());
}
