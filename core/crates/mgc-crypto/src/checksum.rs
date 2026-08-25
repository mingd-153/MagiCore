/// Checksum utilities for quick verification
use anyhow::Result;

/// Compute simple checksum (for quick checks, not cryptographic)
pub fn checksum_adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    const MOD_ADLER: u32 = 65521;

    for &byte in data {
        a = (a + u32::from(byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }

    (b << 16) | a
}

/// Verify adler32 checksum
pub fn verify_adler32(data: &[u8], expected: u32) -> Result<bool> {
    Ok(checksum_adler32(data) == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_adler32_basic() {
        // Known adler32: b"hello" → 0x062c0215
        assert_eq!(checksum_adler32(b"hello"), 0x062c0215);
    }

    #[test]
    fn test_checksum_adler32_empty() {
        // adler32 of empty data: a=1, b=0 → (0 << 16) | 1 = 1
        assert_eq!(checksum_adler32(b""), 1);
    }

    #[test]
    fn test_checksum_adler32_different_inputs_differ() {
        assert_ne!(checksum_adler32(b"abc"), checksum_adler32(b"xyz"));
    }

    #[test]
    fn test_verify_adler32_match() {
        assert!(verify_adler32(b"hello", 0x062c0215).unwrap());
    }

    #[test]
    fn test_verify_adler32_mismatch() {
        assert!(!verify_adler32(b"hello", 0).unwrap());
    }
}
