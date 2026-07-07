use std::ffi::CStr;
use std::os::raw::c_char;

/// Maximum path length in tar headers
const TAR_MAX_PATH: usize = 512;

/// SHA-256 hex string size
const SHA256_HEX_SIZE: usize = 65;

unsafe extern "C" {
    fn mg_tar_extract(
        gz_data: *const u8,
        gz_len: usize,
        callback: extern "C" fn(*mut TarEntryC, *mut std::ffi::c_void) -> i32,
        userdata: *mut std::ffi::c_void,
    ) -> i32;
}

#[repr(C)]
struct TarEntryC {
    path: [c_char; TAR_MAX_PATH],
    data: *mut u8,
    data_len: usize,
    is_executable: i32,
    sha256_hex: [c_char; SHA256_HEX_SIZE],
}

pub struct TarEntry {
    pub path: String,
    pub data: Vec<u8>,
    pub is_executable: bool,
    pub sha256_hex: String,
}

/// Extract gzip-compressed tar archive, returning file entries with SHA-256 hashes.
pub fn extract(gz_data: &[u8]) -> Result<Vec<TarEntry>, String> {
    let entries = std::sync::Mutex::new(Vec::new());
    let error = std::sync::Mutex::new(None::<String>);

    let entries_ptr = &entries as *const _ as *mut std::ffi::c_void;
    let error_ptr = &error as *const _ as *mut std::ffi::c_void;

    // Combined userdata: (entries, error)
    let userdata = Box::into_raw(Box::new((entries_ptr, error_ptr)));

    extern "C" fn callback(
        entry_c: *mut TarEntryC,
        userdata: *mut std::ffi::c_void,
    ) -> i32 {
        let ud = unsafe { &*(userdata as *const (*mut std::ffi::c_void, *mut std::ffi::c_void)) };
        let entries = unsafe { &*(ud.0 as *const std::sync::Mutex<Vec<TarEntry>>) };
        let error = unsafe { &*(ud.1 as *const std::sync::Mutex<Option<String>>) };

        let entry = unsafe { &*entry_c };

        let path = unsafe { CStr::from_ptr(entry.path.as_ptr()) }
            .to_string_lossy()
            .into_owned();

        let sha256_hex = unsafe { CStr::from_ptr(entry.sha256_hex.as_ptr()) }
            .to_string_lossy()
            .into_owned();

        let data = if entry.data_len > 0 && !entry.data.is_null() {
            unsafe { std::slice::from_raw_parts(entry.data, entry.data_len) }.to_vec()
        } else {
            Vec::new()
        };

        let tar_entry = TarEntry {
            path,
            data,
            is_executable: entry.is_executable != 0,
            sha256_hex,
        };

        if let Ok(mut entries) = entries.lock() {
            entries.push(tar_entry);
            0 // continue
        } else {
            if let Ok(mut err) = error.lock() {
                *err = Some("lock poisoned".to_string());
            }
            1 // abort
        }
    }

    unsafe {
        let ret = mg_tar_extract(
            gz_data.as_ptr(),
            gz_data.len(),
            callback,
            userdata as *mut std::ffi::c_void,
        );

        // Free userdata
        let _ = Box::from_raw(userdata);

        if ret != 0 {
            if let Ok(mut err) = error.lock() {
                if let Some(msg) = err.take() {
                    return Err(msg);
                }
            }
            return Err(format!("tar extraction failed with code {}", ret));
        }
    }

    let result = entries
        .lock()
        .map_err(|e| format!("lock error: {}", e))?
        .drain(..)
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar;

    fn create_tarball(files: &[(&str, &str)]) -> Vec<u8> {
        let mut tar_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut tar_data, Compression::default());
            let mut tar_builder = tar::Builder::new(encoder);
            for (name, content) in files {
                let mut header = tar::Header::new_gnu();
                header.set_path(name).unwrap();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar_builder.append(&header, content.as_bytes()).unwrap();
            }
            tar_builder.finish().unwrap();
        }
        tar_data
    }

    #[test]
    fn test_extract_simple() {
        let tar_data = create_tarball(&[
            ("package.json", r#"{"name":"test"}"#),
            ("index.js", "console.log('hello')"),
        ]);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 2);

        let pkg = entries.iter().find(|e| e.path == "package.json").unwrap();
        assert_eq!(pkg.data, br#"{"name":"test"}"#);

        let idx = entries.iter().find(|e| e.path == "index.js").unwrap();
        assert_eq!(idx.data, b"console.log('hello')");
    }

    #[test]
    fn test_extract_sha256() {
        let content = "hello world";
        let tar_data = create_tarball(&[("test.txt", content)]);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 1);

        let expected_hash = crate::cffi::sha256::hash(content.as_bytes());
        assert_eq!(entries[0].sha256_hex, expected_hash);
    }

    #[test]
    fn test_extract_empty_file() {
        let tar_data = create_tarball(&[("empty.txt", "")]);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].data.is_empty());
        // SHA-256 of empty string
        assert_eq!(
            entries[0].sha256_hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_extract_package_prefix() {
        let tar_data = create_tarball(&[
            ("package/package.json", r#"{"name":"test"}"#),
        ]);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "package.json");
    }

    #[test]
    fn test_extract_many_entries() {
        let mut names = Vec::new();
        let mut contents = Vec::new();
        for i in 0..100 {
            names.push(format!("packages/pkg-{}/index.js", i));
            contents.push(format!("module.exports = {{ id: {} }};", i));
        }
        let files: Vec<(&str, &str)> = names.iter().map(|n| n.as_str())
            .zip(contents.iter().map(|c| c.as_str()))
            .collect();
        let tar_data = create_tarball(&files);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 100);

        for (i, entry) in entries.iter().enumerate() {
            let expected_path = format!("packages/pkg-{}/index.js", i);
            assert_eq!(entry.path, expected_path);
            let expected_content = format!("module.exports = {{ id: {} }};", i);
            assert_eq!(String::from_utf8_lossy(&entry.data), expected_content);
        }
    }

    #[test]
    fn test_extract_many_entries_1000() {
        let count = 1000;
        let mut names = Vec::new();
        let mut contents = Vec::new();
        for i in 0..count {
            names.push(format!("node_modules/pkg-{}/index.js", i));
            contents.push(format!("module.exports = {{ id: {} }};", i));
        }
        let files: Vec<(&str, &str)> = names.iter().map(|n| n.as_str())
            .zip(contents.iter().map(|c| c.as_str()))
            .collect();
        let tar_data = create_tarball(&files);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), count);

        for i in 0..5 {
            assert_eq!(entries[i].path, format!("node_modules/pkg-{}/index.js", i));
        }
    }

    #[test]
    fn test_extract_binary_data() {
        let mut tar_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut tar_data, Compression::default());
            let mut tar_builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_path("data.bin").unwrap();
            let bin: Vec<u8> = (0..255).collect();
            header.set_size(bin.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder.append(&header, &bin[..]).unwrap();
            tar_builder.finish().unwrap();
        }

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "data.bin");
        assert_eq!(entries[0].data.len(), 255);
        for (i, &b) in entries[0].data.iter().enumerate() {
            assert_eq!(b, i as u8, "byte {} mismatch", i);
        }
    }

    #[test]
    fn test_extract_deep_path() {
        let tar_data = create_tarball(&[
            ("a/very/deep/nested/directory/structure/file.txt", "deep"),
        ]);

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a/very/deep/nested/directory/structure/file.txt");
        assert_eq!(entries[0].data, b"deep");
    }

    #[test]
    fn test_extract_mixed_executable() {
        let mut tar_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut tar_data, Compression::default());
            let mut tar_builder = tar::Builder::new(encoder);

            let mut h1 = tar::Header::new_gnu();
            h1.set_path("readme.txt").unwrap();
            h1.set_size(4);
            h1.set_mode(0o644);
            h1.set_cksum();
            tar_builder.append(&h1, &b"text"[..]).unwrap();

            let mut h2 = tar::Header::new_gnu();
            h2.set_path("run.sh").unwrap();
            h2.set_size(3);
            h2.set_mode(0o755);
            h2.set_cksum();
            tar_builder.append(&h2, &b"bin"[..]).unwrap();

            let mut h3 = tar::Header::new_gnu();
            h3.set_path("lib.js").unwrap();
            h3.set_size(6);
            h3.set_mode(0o644);
            h3.set_cksum();
            tar_builder.append(&h3, &b"export"[..]).unwrap();

            tar_builder.finish().unwrap();
        }

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(!entries[0].is_executable, "readme.txt should not be executable");
        assert!(entries[1].is_executable, "run.sh should be executable");
        assert!(!entries[2].is_executable, "lib.js should not be executable");
    }

    #[test]
    fn test_extract_executable() {
        let mut tar_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut tar_data, Compression::default());
            let mut tar_builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_path("script.sh").unwrap();
            header.set_size(5);
            header.set_mode(0o755); // executable
            header.set_cksum();
            tar_builder.append(&header, &b"echo 1"[..]).unwrap();
            tar_builder.finish().unwrap();
        }

        let entries = extract(&tar_data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_executable);
    }
}
