/// Tarball builder — deterministic (mtime epoch, sorted entries) + sha1/sha512 (01 §4.6)
use crate::ignore::select_files;
use anyhow::Result;
use base64::Engine;
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct PackResult {
    pub tarball_path: std::path::PathBuf,
    pub shasum: String,
    pub integrity: String,
    pub size: u64,
    pub unpacked_size: u64,
    pub entry_count: usize,
}

/// Đóng gói root → tarball gzip tại output_path.
/// Deterministic: mtime = 0 (epoch), entries sorted, gzip mtime = 0 (flate2 Header mtime=0 mặc định).
pub fn pack(root: &Path, output_path: &Path, prefix: &str) -> Result<PackResult> {
    let files = select_files(root)?;
    let mut unpacked_size: u64 = 0;
    let file = File::create(output_path)?;
    let mut gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    {
        let mut builder = tar::Builder::new(&mut gz);
        builder.follow_symlinks(false);
        for rel in &files {
            let src = root.join(rel);
            let entry_name = if prefix.is_empty() {
                rel.to_string_lossy().to_string()
            } else {
                format!("{}/{}", prefix, rel.to_string_lossy())
            };
            let mut data = Vec::new();
            File::open(&src)?.read_to_end(&mut data)?;
            unpacked_size += data.len() as u64;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0); // deterministic — mtime epoch
            header.set_cksum();
            builder.append_data(&mut header, &entry_name, data.as_slice())?;
        }
        builder.finish()?;
    }
    gz.finish()?;

    // hashes + size
    let mut data = Vec::new();
    File::open(output_path)?.read_to_end(&mut data)?;
    let sha1 = Sha1::digest(&data);
    let sha512 = Sha512::digest(&data);
    let shasum = hex::encode(sha1);
    let integrity = format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(sha512)
    );
    let size = data.len() as u64;

    Ok(PackResult {
        tarball_path: output_path.to_path_buf(),
        shasum,
        integrity,
        size,
        unpacked_size,
        entry_count: files.len(),
    })
}
