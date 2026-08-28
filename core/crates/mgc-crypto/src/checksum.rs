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

