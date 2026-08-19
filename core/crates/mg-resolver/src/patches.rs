//! Patch apply engine — parse unified diff, apply to vstore (16 §5)
//! (Apply engine nội bộ — KHÔNG gọi binary `patch`, cross-platform, chạy offline)

use anyhow::{anyhow, bail, Context, Result};
use mg_platform::paths::{GlobalPaths, ProjectPaths};
use std::fs;
use std::path::{Path, PathBuf};

/// Apply a unified diff patch to a vstore directory.
/// Returns the list of modified files.
pub fn apply_patch(vstore_root: &Path, patch_path: &Path) -> Result<Vec<PathBuf>> {
    let content = fs::read_to_string(patch_path)
        .with_context(|| format!("read patch file {}", patch_path.display()))?;
    let mut modified = Vec::new();
    let mut lines = content.lines().map(|s| s.to_string()).peekable();

    while lines.peek().is_some() {
        // Parse file header: --- a/file and +++ b/file
        let old_file = parse_file_header(&mut lines, "---")?;
        let new_file = parse_file_header(&mut lines, "+++")?;

        if old_file != new_file {
            bail!("patch file mismatch: {} vs {}", old_file, new_file);
        }

        let target = vstore_root.join(&old_file);
        if !target.exists() {
            bail!("target file not found in vstore: {}", target.display());
        }

        let mut hunks = Vec::new();
        while let Some(line) = lines.next() {
            if line.starts_with("@@") {
                hunks.push(parse_hunk(line, &mut lines)?);
            } else if line.starts_with("---") || line.starts_with("+++") {
                // Next file header - put back
                break;
            }
        }

        apply_hunks(&target, hunks)?;
        modified.push(target);
    }

    Ok(modified)
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
    Ok(path.to_string())
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
    while let Some(line) = lines.next() {
        if line.starts_with("@@") || line.starts_with("---") || line.starts_with("+++") {
            // Put back - will be processed in next iteration
            break;
        }
        if line.is_empty() {
            hunk_lines.push((' ', String::new()));
            continue;
        }
        let op = line.chars().next().unwrap_or(' ');
        let content = &line[1..];
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

fn apply_hunks(target: &Path, hunks: Vec<Hunk>) -> Result<()> {
    let content = fs::read_to_string(target)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    for hunk in hunks {
        // Convert 1-based to 0-based
        let start = hunk.old_start.saturating_sub(1);
        let end = start + hunk.old_len;

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

    fs::write(target, lines.join("\n"))?;
    Ok(())
}

/// Verify patch integrity (SHA256 of patch file content)
pub fn verify_patch_integrity(patch_path: &Path, expected_sha256: &str) -> Result<bool> {
    use sha2::{Digest, Sha256};
    let content = fs::read(patch_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let actual = hex::encode(hasher.finalize());
    Ok(actual == expected_sha256)
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
