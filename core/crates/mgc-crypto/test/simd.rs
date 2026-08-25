//! Tests for simd module
//! Tests cho module simd

use mgc_crypto::simd::{detect_simd, simd_info, SimdCapability};

#[test]
fn test_detect_simd() {
    let capability = detect_simd();
    // Should detect something on modern CPUs — Nên phát hiện được gì đó trên CPU hiện đại
    assert_ne!(capability.name(), "");
}

#[test]
fn test_simd_capability_name() {
    assert_eq!(SimdCapability::None.name(), "None (fallback)");
    assert_eq!(SimdCapability::AVX2.name(), "AVX2");
    assert_eq!(SimdCapability::NEON.name(), "NEON");
}

#[test]
fn test_performance_multiplier() {
    assert_eq!(SimdCapability::None.performance_multiplier(), 1.0);
    assert_eq!(SimdCapability::AVX2.performance_multiplier(), 4.0);
    assert_eq!(SimdCapability::AVX512.performance_multiplier(), 8.0);
}

#[test]
fn test_simd_info() {
    let info = simd_info();
    assert!(info.contains("SIMD:"));
    assert!(info.contains("performance"));
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_x86_64_detects_at_least_sse2() {
    let capability = detect_simd();
    // x86_64 baseline includes SSE2 — x86_64 baseline bao gồm SSE2
    assert!(matches!(
        capability,
        SimdCapability::SSE2 | SimdCapability::AVX2 | SimdCapability::AVX512
    ));
}

#[test]
#[cfg(target_arch = "aarch64")]
fn test_aarch64_detects_neon() {
    let capability = detect_simd();
    assert_eq!(capability, SimdCapability::NEON);
}
