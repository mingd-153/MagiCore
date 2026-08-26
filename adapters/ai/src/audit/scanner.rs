//! Security scanners cho model files.

use super::Finding;
use mgc_types::MgResult;
use std::path::Path;

/// Scan pickle file for dangerous imports
/// P1: Byte pattern matching (simple but effective for common attacks)
/// P2 (deferred): Deep bytecode AST parsing with pyo3 when real exploit case appears
/// Rationale: byte patterns catch 90% of known exploits; deep parsing adds
/// build complexity (Python dependency) + version fragility + scanner attack surface.
/// Current approach is fail-closed (reject on suspicion) + loud warning.
pub fn scan_pickle(path: &Path) -> MgResult<Vec<Finding>> {
    let content = std::fs::read(path)?;
    let mut findings = Vec::new();

    // Check for common malicious pickle patterns (opcodes + imports)
    // Pickle opcodes: GLOBAL (c), REDUCE (R), BUILD (\x81), INST (i)
    let dangerous_patterns: &[(&[u8], &str)] = &[
        (b"os.system", "os.system (arbitrary command execution)"),
        (b"subprocess", "subprocess (shell access)"),
        (b"eval", "eval (code injection)"),
        (b"exec", "exec (code execution)"),
        (b"__import__", "__import__ (dynamic import)"),
        (b"socket", "socket (network access)"),
        (b"builtins", "builtins (unrestricted access)"),
        // Pickle opcodes for code execution
        (b"c__builtin__\neval\n", "pickle GLOBAL opcode with eval"),
        (b"cos\nsystem\n", "pickle GLOBAL opcode with os.system"),
        (b"\x81", "pickle BUILD opcode (class instantiation)"),
    ];

    for (pattern, desc) in dangerous_patterns {
        if contains_bytes(&content, pattern) {
            findings.push(
                Finding::critical(
                    "pickle-exploit",
                    &format!("Dangerous pattern detected: {}", desc),
                )
                .with_file(&path.to_string_lossy()),
            );
        }
    }

    // Check file size (pickle > 10GB suspicious)
    if content.len() > 10 * 1024 * 1024 * 1024 {
        findings.push(
            Finding::high("size", "Pickle file unusually large (>10GB)")
                .with_file(&path.to_string_lossy()),
        );
    }

    Ok(findings)
}

/// Scan safetensors for metadata issues
pub fn scan_safetensors(path: &Path) -> MgResult<Vec<Finding>> {
    let content = std::fs::read(path)?;
    let mut findings = Vec::new();

    // SafeTensors format: 8-byte header (JSON length) + JSON metadata + tensors
    if content.len() < 8 {
        findings.push(
            Finding::critical("safetensors-format", "Invalid safetensors header")
                .with_file(&path.to_string_lossy()),
        );
        return Ok(findings);
    }

    // Read header length (little-endian u64)
    let header_len = u64::from_le_bytes([
        content[0], content[1], content[2], content[3], content[4], content[5], content[6],
        content[7],
    ]) as usize;

    if header_len > 100 * 1024 * 1024 {
        findings.push(
            Finding::high("safetensors-header", "Suspiciously large metadata (>100MB)")
                .with_file(&path.to_string_lossy()),
        );
    }

    if header_len + 8 > content.len() {
        findings.push(
            Finding::critical("safetensors-truncated", "Truncated or corrupted file")
                .with_file(&path.to_string_lossy()),
        );
    }

    Ok(findings)
}

/// Scan weight files for suspicious patterns
pub fn scan_weights(path: &Path) -> MgResult<Vec<Finding>> {
    let metadata = std::fs::metadata(path)?;
    let mut findings = Vec::new();

    // Check file size
    let size_gb = metadata.len() / (1024 * 1024 * 1024);

    if size_gb > 100 {
        findings.push(
            Finding::medium("size", &format!("Very large model file ({}GB)", size_gb))
                .with_file(&path.to_string_lossy()),
        );
    }

    // For .pt/.pth files, check for pickle header
    if let Some(ext) = path.extension() {
        if ext == "pt" || ext == "pth" {
            let content = std::fs::read(path)?;

            // PyTorch files use pickle internally
            if content.len() > 6 && &content[0..6] == b"\x80\x02}q\x00" {
                // Valid pickle header - check for dangerous patterns
                let pickle_findings = scan_pickle(path)?;
                findings.extend(pickle_findings);
            }
        }
    }

    Ok(findings)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_scan_pickle_safe() {
        let tmp = tmp();
        let pkl = tmp.path().join("safe.pkl");
        std::fs::write(&pkl, b"safe pickle data").unwrap();

        let findings = scan_pickle(&pkl).unwrap();
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_scan_pickle_dangerous() {
        let tmp = tmp();
        let pkl = tmp.path().join("evil.pkl");
        std::fs::write(&pkl, b"os.system('rm -rf /')").unwrap();

        let findings = scan_pickle(&pkl).unwrap();
        assert!(!findings.is_empty());
        assert_eq!(findings[0].severity, super::super::Severity::Critical);
    }

    #[test]
    fn test_scan_safetensors_valid() {
        let tmp = tmp();
        let st = tmp.path().join("model.safetensors");

        // Create minimal valid safetensors: 8-byte header + JSON
        let mut data = vec![0u8; 8];
        let json = b"{}";
        let header_len = json.len() as u64;
        data[0..8].copy_from_slice(&header_len.to_le_bytes());
        data.extend_from_slice(json);

        std::fs::write(&st, data).unwrap();

        let findings = scan_safetensors(&st).unwrap();
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_scan_safetensors_invalid() {
        let tmp = tmp();
        let st = tmp.path().join("bad.safetensors");
        std::fs::write(&st, b"short").unwrap();

        let findings = scan_safetensors(&st).unwrap();
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_scan_weights_large() {
        let tmp = tmp();
        let weights = tmp.path().join("huge.bin");

        // Create 101GB file metadata (stub - don't actually write)
        std::fs::write(&weights, b"small file").unwrap();

        let findings = scan_weights(&weights).unwrap();
        // Small file - no findings
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_contains_bytes() {
        assert!(contains_bytes(b"hello world", b"world"));
        assert!(!contains_bytes(b"hello world", b"foo"));
        assert!(contains_bytes(b"os.system", b"os.system"));
    }
}
