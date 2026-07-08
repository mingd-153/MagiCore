//! Fuzz target for registry response parsing
//!
//! Tests that JSON registry responses parse without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the registry response parser with random bytes as JSON
    let _ = serde_json::from_slice::<serde_json::Value>(data);
    // Try parsing as a generic registry package metadata structure
    let _ = serde_json::from_slice::<mg_core::PackageMetadata>(data);
});
