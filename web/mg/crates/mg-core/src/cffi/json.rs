/* C API declarations */
extern "C" {
    fn mg_json_get_string(
        json: *const std::os::raw::c_char,
        key: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        out_len: usize,
    ) -> std::os::raw::c_int;

    fn mg_json_get_int(
        json: *const std::os::raw::c_char,
        key: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_int,
    ) -> std::os::raw::c_int;

    fn mg_json_iterate_versions(
        json: *const std::os::raw::c_char,
        cb: Option<
            unsafe extern "C" fn(
                *const std::os::raw::c_char,
                usize,
                *const std::os::raw::c_char,
                usize,
                *mut std::os::raw::c_void,
            ) -> std::os::raw::c_int,
        >,
        ctx: *mut std::os::raw::c_void,
    ) -> std::os::raw::c_int;

    fn mg_json_iterate_deps(
        json: *const std::os::raw::c_char,
        version: *const std::os::raw::c_char,
        cb: Option<
            unsafe extern "C" fn(
                *const std::os::raw::c_char,
                usize,
                *const std::os::raw::c_char,
                usize,
                *mut std::os::raw::c_void,
            ) -> std::os::raw::c_int,
        >,
        ctx: *mut std::os::raw::c_void,
    ) -> std::os::raw::c_int;
}

/// Extract a string field from JSON using C implementation.
/// Returns None if the field is not found or an error occurs.
pub fn get_string(json: &str, key: &str) -> Option<String> {
    let json_cstr = std::ffi::CString::new(json).ok()?;
    let key_cstr = std::ffi::CString::new(key).ok()?;
    let mut buf = vec![0u8; 4096];

    let ret = unsafe {
        mg_json_get_string(
            json_cstr.as_ptr(),
            key_cstr.as_ptr(),
            buf.as_mut_ptr() as *mut std::os::raw::c_char,
            buf.len(),
        )
    };

    if ret == 0 {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..len]).into_owned())
    } else {
        None
    }
}

/// Extract an integer field from JSON using C implementation.
pub fn get_int(json: &str, key: &str) -> Option<i32> {
    let json_cstr = std::ffi::CString::new(json).ok()?;
    let key_cstr = std::ffi::CString::new(key).ok()?;
    let mut val: std::os::raw::c_int = 0;

    let ret = unsafe { mg_json_get_int(json_cstr.as_ptr(), key_cstr.as_ptr(), &mut val) };
    if ret == 0 { Some(val) } else { None }
}

/// Iterate version keys from a registry JSON response.
pub fn iterate_versions(json: &str) -> Vec<String> {
    let json_cstr = match std::ffi::CString::new(json) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let versions = std::sync::Mutex::new(Vec::new());

    unsafe extern "C" fn version_cb(
        key: *const std::os::raw::c_char,
        key_len: usize,
        _val: *const std::os::raw::c_char,
        _val_len: usize,
        ctx: *mut std::os::raw::c_void,
    ) -> std::os::raw::c_int {
        let versions = &*(ctx as *mut std::sync::Mutex<Vec<String>>);
        let s = std::str::from_utf8(std::slice::from_raw_parts(key as *const u8, key_len))
            .unwrap_or("")
            .to_string();
        if let Ok(mut v) = versions.lock() {
            v.push(s);
        }
        0
    }

    let ctx = &versions as *const _ as *mut std::os::raw::c_void;
    unsafe {
        mg_json_iterate_versions(json_cstr.as_ptr(), Some(version_cb), ctx);
    }

    versions.into_inner().unwrap_or_default()
}

/// Iterate dependencies from a specific version in registry JSON.
pub fn iterate_deps(json: &str, version: &str) -> Vec<(String, String)> {
    let json_cstr = match std::ffi::CString::new(json) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let ver_cstr = match std::ffi::CString::new(version) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let deps = std::sync::Mutex::new(Vec::new());

    unsafe extern "C" fn dep_cb(
        key: *const std::os::raw::c_char,
        key_len: usize,
        val: *const std::os::raw::c_char,
        val_len: usize,
        ctx: *mut std::os::raw::c_void,
    ) -> std::os::raw::c_int {
        let deps = &*(ctx as *mut std::sync::Mutex<Vec<(String, String)>>);
        let k = std::str::from_utf8(std::slice::from_raw_parts(key as *const u8, key_len))
            .unwrap_or("")
            .to_string();
    let v = if val_len > 0 {
        std::str::from_utf8(std::slice::from_raw_parts(val as *const u8, val_len))
            .unwrap_or("")
            .trim_matches('"')
            .to_string()
    } else {
        String::new()
    };
        if let Ok(mut d) = deps.lock() {
            d.push((k, v));
        }
        0
    }

    let ctx = &deps as *const _ as *mut std::os::raw::c_void;
    unsafe {
        mg_json_iterate_deps(json_cstr.as_ptr(), ver_cstr.as_ptr(), Some(dep_cb), ctx);
    }

    deps.into_inner().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry_json() -> &'static str {
        r#"{
            "name": "react",
            "description": "React is a JavaScript library for building user interfaces.",
            "dist-tags": { "latest": "18.2.0" },
            "versions": {
                "18.2.0": {
                    "name": "react",
                    "version": "18.2.0",
                    "dependencies": {
                        "loose-envify": "^1.1.0"
                    }
                },
                "19.0.0": {
                    "name": "react",
                    "version": "19.0.0",
                    "dependencies": {}
                }
            }
        }"#
    }

    #[test]
    fn test_get_string() {
        let json = sample_registry_json();
        assert_eq!(get_string(json, "name").as_deref(), Some("react"));
        assert_eq!(get_string(json, "description").as_deref(),
            Some("React is a JavaScript library for building user interfaces."));
    }

    #[test]
    fn test_get_string_missing() {
        assert_eq!(get_string(r#"{"a": 1}"#, "b"), None);
    }

    #[test]
    fn test_iterate_versions() {
        let json = sample_registry_json();
        let versions = iterate_versions(json);
        assert!(versions.contains(&"18.2.0".to_string()));
        assert!(versions.contains(&"19.0.0".to_string()));
    }

    #[test]
    fn test_iterate_deps() {
        let json = sample_registry_json();
        let deps = iterate_deps(json, "18.2.0");
        assert!(deps.contains(&("loose-envify".to_string(), "^1.1.0".to_string())));
    }

    #[test]
    fn test_iterate_deps_empty() {
        let json = sample_registry_json();
        let deps = iterate_deps(json, "19.0.0");
        assert!(deps.is_empty() || deps.is_empty());
    }
}
