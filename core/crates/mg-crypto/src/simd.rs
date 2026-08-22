//! SIMD capability detection
//! Phát hiện khả năng SIMD

use serde::{Deserialize, Serialize};

/// SIMD capability — Khả năng SIMD
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimdCapability {
    /// No SIMD (fallback) — Không SIMD (fallback)
    None,
    /// SSE2 (x86/x86_64 baseline) — SSE2 (baseline x86)
    SSE2,
    /// AVX2 (x86/x86_64 modern) — AVX2 (x86 hiện đại)
    AVX2,
    /// AVX-512 (x86/x86_64 high-end) — AVX-512 (x86 cao cấp)
    AVX512,
    /// NEON (ARM) — NEON (ARM)
    NEON,
}

impl SimdCapability {
    /// Get human-readable name — Lấy tên dễ đọc
    pub fn name(&self) -> &'static str {
        match self {
            SimdCapability::None => "None (fallback)",
            SimdCapability::SSE2 => "SSE2",
            SimdCapability::AVX2 => "AVX2",
            SimdCapability::AVX512 => "AVX-512",
            SimdCapability::NEON => "NEON",
        }
    }

    /// Get expected performance multiplier vs fallback
    /// Lấy hệ số hiệu năng dự kiến so với fallback
    pub fn performance_multiplier(&self) -> f64 {
        match self {
            SimdCapability::None => 1.0,
            SimdCapability::SSE2 => 2.0,
            SimdCapability::AVX2 => 4.0,
            SimdCapability::AVX512 => 8.0,
            SimdCapability::NEON => 3.0,
        }
    }
}

/// Detect SIMD capability at runtime — Phát hiện SIMD runtime
pub fn detect_simd() -> SimdCapability {
    // BLAKE3 crate auto-detects and uses best SIMD available
    // We just report what's available on this CPU
    // BLAKE3 crate tự động phát hiện và dùng SIMD tốt nhất
    // Chúng ta chỉ báo cáo có gì trên CPU này

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f") {
            return SimdCapability::AVX512;
        }
        if is_x86_feature_detected!("avx2") {
            return SimdCapability::AVX2;
        }
        if is_x86_feature_detected!("sse2") {
            return SimdCapability::SSE2;
        }
    }

    #[cfg(target_arch = "x86")]
    {
        if is_x86_feature_detected!("avx2") {
            return SimdCapability::AVX2;
        }
        if is_x86_feature_detected!("sse2") {
            return SimdCapability::SSE2;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // NEON is baseline on AArch64 — NEON là baseline trên AArch64
        return SimdCapability::NEON;
    }

    #[cfg(all(target_arch = "arm", target_feature = "neon"))]
    {
        return SimdCapability::NEON;
    }

    // A7 FIX: Remove unreachable code warning
    // Fallback if no SIMD detected — Fallback nếu không phát hiện SIMD
    #[allow(unreachable_code)]
    SimdCapability::None
}

/// Get SIMD info string — Lấy chuỗi thông tin SIMD
pub fn simd_info() -> String {
    let capability = detect_simd();
    format!(
        "SIMD: {} ({}x performance)",
        capability.name(),
        capability.performance_multiplier()
    )
}
