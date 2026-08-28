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
#[path = "test/scanner_tests.rs"]
mod tests;
