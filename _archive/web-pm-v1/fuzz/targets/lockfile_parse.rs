//! Fuzz target for lockfile parsing
//!
//! Tests that the lockfile parser handles arbitrary input without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Test text lockfile parsing with arbitrary input
        let _ = mg_lockfile::text::read_text(&std::path::Path::new(s));
        // Test lockfile path preference
        mg_lockfile::text::get_preferred_path(&std::path::Path::new(s));
    }
});
