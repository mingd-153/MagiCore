unsafe extern "C" {
    fn mg_sha256_init(ctx: *mut mg_sha256_ctx_t);
    fn mg_sha256_update(ctx: *mut mg_sha256_ctx_t, data: *const std::os::raw::c_void, len: usize);
    fn mg_sha256_final_hex(ctx: *mut mg_sha256_ctx_t, out: *mut std::os::raw::c_char);
    fn mg_sha256_final_raw(ctx: *mut mg_sha256_ctx_t, out: *mut u8);
    fn mg_sha256_hash(data: *const std::os::raw::c_void, len: usize, out: *mut std::os::raw::c_char);
}

#[repr(C)]
struct mg_sha256_ctx_t {
    count: u64,
    state: [u32; 8],
    buffer: [u8; 64],
}

/// Compute SHA-256 hex digest using C implementation.
pub fn hash(data: &[u8]) -> String {
    let mut out = [0u8; 65];
    unsafe {
        mg_sha256_hash(
            data.as_ptr() as *const std::os::raw::c_void,
            data.len(),
            out.as_mut_ptr() as *mut std::os::raw::c_char,
        );
    }
    let len = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    String::from_utf8_lossy(&out[..len]).into_owned()
}

/// Streaming SHA-256 hasher.
pub struct Hasher {
    ctx: mg_sha256_ctx_t,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        let mut ctx = mg_sha256_ctx_t {
            count: 0,
            state: [0; 8],
            buffer: [0; 64],
        };
        unsafe { mg_sha256_init(&mut ctx) };
        Self { ctx }
    }

    pub fn update(&mut self, data: &[u8]) {
        unsafe {
            mg_sha256_update(
                &mut self.ctx,
                data.as_ptr() as *const std::os::raw::c_void,
                data.len(),
            );
        }
    }

    pub fn final_hex(&mut self) -> String {
        let mut out = [0u8; 65];
        unsafe {
            mg_sha256_final_hex(
                &mut self.ctx,
                out.as_mut_ptr() as *mut std::os::raw::c_char,
            );
        }
        let len = out.iter().position(|&c| c == 0).unwrap_or(out.len());
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    pub fn final_raw(&mut self) -> [u8; 32] {
        let mut out = [0u8; 32];
        unsafe {
            mg_sha256_final_raw(&mut self.ctx, out.as_mut_ptr());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        // SHA-256 of empty string
        assert_eq!(hash(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_sha256_hello() {
        assert_eq!(hash(b"hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_sha256_streaming() {
        let mut h = Hasher::new();
        h.update(b"hello");
        h.update(b" ");
        h.update(b"world");
        assert_eq!(h.final_hex(), "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }
}
