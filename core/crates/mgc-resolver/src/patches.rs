//! Patch apply engine — parse unified diff, apply to vstore (16 §5)
//! (Apply engine nội bộ — KHÔNG gọi binary `patch`, cross-platform, chạy offline)

use anyhow::{Context, Result, anyhow, bail};
use mgc_platform::paths::{GlobalPaths, ProjectPaths};
use std::fs;
use std::path::{Path, PathBuf};

/// Apply a unified diff patch to a vstore directory.
/// Returns the list of modified files.
pub fn apply_patch(vstore_root: &Path, patch_path: &Path) -> Result<Vec<PathBuf>> {
    let content = fs::read_to_string(patch_path)
        .with_context(|| format!("read patch file {}", patch_path.display()))?;
    let mut staged = Vec::new();
    let mut lines = content.lines().map(|s| s.to_string()).peekable();

    while lines.peek().is_some() {
        // Parse file header: --- a/file and +++ b/file
        let old_file = parse_file_header(&mut lines, "---")?;
        let new_file = parse_file_header(&mut lines, "+++")?;

        if old_file != new_file {
            bail!("patch file mismatch: {} vs {}", old_file, new_file);
        }

        let target = safe_vstore_target(vstore_root, &old_file)?;
        if !target.exists() {
            bail!("target file not found in vstore: {}", target.display());
        }

        let mut hunks = Vec::new();
        while let Some(line) = lines.peek() {
            if line.starts_with("---") {
                break;
            }
            if !line.starts_with("@@") {
                bail!("expected unified-diff hunk header, found: {line}");
            }
            let Some(header) = lines.next() else {
                bail!("patch stream ended after a peeked hunk header");
            };
            hunks.push(parse_hunk(header, &mut lines)?);
        }
        let original = fs::read_to_string(&target)?;
        let updated = apply_hunks(&original, hunks)?;
        staged.push((target, updated));
    }

    // Parse every file and validate every hunk before the first write. This
    // prevents a malformed later file from leaving earlier files modified.
    // (Đọc/kiểm tra mọi file trước lần ghi đầu tiên.)
    for (target, updated) in &staged {
        fs::write(target, updated)?;
    }
    Ok(staged.into_iter().map(|(target, _)| target).collect())
}

#[derive(Debug)]
struct Hunk {
    old_start: usize,
    old_len: usize,
    lines: Vec<(char, String)>, // (' ', '-', '+') + content
}

fn parse_file_header<I>(lines: &mut std::iter::Peekable<I>, prefix: &str) -> Result<String>
where
    I: Iterator<Item = std::string::String>,
{
    let line = lines
        .next()
        .ok_or_else(|| anyhow!("expected {} header", prefix))?;
    if !line.starts_with(prefix) {
        bail!("expected line starting with {}", prefix);
    }
    // Format: --- a/path/to/file
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("invalid {} header", prefix);
    }
    // Strip 'a/' or 'b/' prefix
    let path = parts[1].trim_start_matches("a/").trim_start_matches("b/");
    validate_patch_relative_path(path)?;
    Ok(path.to_string())
}

/// Reject patch headers that could address files outside the package store.
/// (Từ chối header patch có thể ghi file bên ngoài package store.)
fn validate_patch_relative_path(path: &str) -> Result<()> {
    use std::path::Component;

    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        bail!("unsafe patch path: expected a non-empty slash-separated relative path");
    }
    let candidate = Path::new(path);
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) | Component::CurDir
        )
    }) {
        bail!("unsafe patch path: path traversal or absolute paths are forbidden: {path}");
    }
    Ok(())
}

/// Canonicalize both sides so symlinked package files cannot escape vstore.
/// (Canonicalize hai phía để symlink trong package không thoát vstore.)
fn safe_vstore_target(vstore_root: &Path, relative_path: &str) -> Result<PathBuf> {
    validate_patch_relative_path(relative_path)?;
    let root = vstore_root.canonicalize()?;
    let target = root.join(relative_path);
    let canonical_target = target.canonicalize()?;
    if !canonical_target.starts_with(&root) {
        bail!("patch target escapes package store: {}", target.display());
    }
    if !canonical_target.is_file() {
        bail!("patch target is not a regular file: {}", target.display());
    }
    Ok(canonical_target)
}

fn parse_hunk<I>(header: String, lines: &mut std::iter::Peekable<I>) -> Result<Hunk>
where
    I: Iterator<Item = std::string::String>,
{
    // @@ -start,len +start,len @@
    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() < 3 {
        bail!("invalid hunk header: {}", header);
    }
    let old_range = parse_range(parts[1])?;
    let _new_range = parse_range(parts[2])?;

    let mut hunk_lines = Vec::new();
    while let Some(line) = lines.peek() {
        if line.starts_with("@@") || line.starts_with("---") || line.starts_with("+++") {
            // Leave the next hunk/file header for the caller.
            break;
        }
        let Some(line) = lines.next() else {
            bail!("patch stream ended after a peeked patch line");
        };
        if line.is_empty() {
            hunk_lines.push((' ', String::new()));
            continue;
        }
        let op = line.chars().next().unwrap_or(' ');
        if !matches!(op, ' ' | '+' | '-') {
            bail!("invalid hunk operation: {op:?}");
        }
        let content = line
            .get(op.len_utf8()..)
            .ok_or_else(|| anyhow!("invalid UTF-8 boundary in patch hunk"))?;
        hunk_lines.push((op, content.to_string()));
    }

    Ok(Hunk {
        old_start: old_range.0,
        old_len: old_range.1,
        lines: hunk_lines,
    })
}

fn parse_range(s: &str) -> Result<(usize, usize)> {
    // Format: -start,len or +start,len
    let s = s.trim_start_matches('-').trim_start_matches('+');
    let parts: Vec<&str> = s.split(',').collect();
    let start = parts[0].parse::<usize>()?;
    let len = if parts.len() > 1 {
        parts[1].parse::<usize>()?
    } else {
        1
    };
    Ok((start, len))
}

fn apply_hunks(content: &str, hunks: Vec<Hunk>) -> Result<String> {
    let trailing_newline = content.ends_with('\n');
    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    for hunk in hunks {
        // Convert 1-based to 0-based
        let start = hunk.old_start.saturating_sub(1);
        let end = start
            .checked_add(hunk.old_len)
            .ok_or_else(|| anyhow!("hunk range overflows addressable file length"))?;

        if end > lines.len() {
            bail!("hunk range exceeds file length: {} > {}", end, lines.len());
        }

        // Verify context lines match
        let mut expected_old = Vec::new();
        for (op, content) in &hunk.lines {
            match op {
                ' ' | '-' => expected_old.push(content.clone()),
                _ => {}
            }
        }

        let actual_old: Vec<String> = lines[start..end].to_vec();
        if actual_old != expected_old {
            bail!(
                "patch context mismatch at line {}: expected {:?}, got {:?}",
                start + 1,
                expected_old,
                actual_old
            );
        }

        // Build new lines
        let mut new_lines = Vec::new();
        for (op, content) in &hunk.lines {
            match op {
                ' ' | '+' => new_lines.push(content.clone()),
                '-' => {} // deleted
                _ => bail!("invalid hunk operation: {}", op),
            }
        }

        // Replace
        lines.splice(start..end, new_lines);
    }

    let mut updated = lines.join(line_ending);
    if trailing_newline {
        updated.push_str(line_ending);
    }
    Ok(updated)
}

/// Verify patch integrity (SHA256 of patch file content)
pub fn verify_patch_integrity(patch_path: &Path, expected_sha256: &str) -> Result<bool> {
    use sha2::{Digest, Sha256};
    let content = fs::read(patch_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let actual = hex::encode(hasher.finalize());
    // Accept the SRI form (`sha256-<hex>`, what `mgc patch add` records)
    // as well as bare hex — comparing raw against prefixed can never
    // match, which made every added patch fail verification.
    // (Chấp nhận cả dạng SRI lẫn hex trần.)
    let expected = expected_sha256
        .strip_prefix("sha256-")
        .unwrap_or(expected_sha256);
    Ok(actual == expected)
}

/// Get patches directory from project or global
pub fn get_patches_dir(project_root: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = project_root {
        let paths = ProjectPaths::from_root(root);
        Ok(paths.patches_dir().to_path_buf())
    } else {
        let paths = GlobalPaths::new()?;
        Ok(paths.patches_dir().to_path_buf())
    }
}
