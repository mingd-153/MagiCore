/// File selection — .npmignore (ưu tiên) / .gitignore + luôn thêm/bỏ (01 §4.5)
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Luôn thêm vào tarball nếu tồn tại
const ALWAYS_INCLUDE: &[&str] = &["package.json", "README.md", "LICENSE", "CHANGELOG.md"];
/// Luôn bỏ khỏi tarball
const ALWAYS_EXCLUDE: &[&str] = &["node_modules", ".git", "mg.lock", ".megagate"];

/// Danh sách file sẽ đóng gói (relative path, phân tách `/`).
pub fn select_files(root: &Path) -> Result<Vec<PathBuf>> {
    let npmignore = root.join(".npmignore");
    let has_npmignore = npmignore.exists();

    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false); // đóng gói file ẩn (trừ luật ignore) — đúng npm behavior
    builder.require_git(false); // đọc .gitignore kể cả không có .git dir
    if has_npmignore {
        // npm behavior: .npmignore THAY THẾ .gitignore (không cộng dồn)
        builder.git_ignore(false);
        builder.git_global(false);
        builder.git_exclude(false);
        builder.add_custom_ignore_filename(".npmignore");
    } else {
        builder.ignore(true); // .gitignore + .ignore
        builder.git_ignore(true);
        builder.git_global(true);
        builder.git_exclude(true);
    }

    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for entry in builder.build() {
        let entry = entry?;
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(path);
        if should_include(rel) && seen.insert(rel.to_path_buf()) {
            files.push(rel.to_path_buf());
        }
    }

    // Luôn thêm file cần thiết (ngay cả khi bị ignore)
    for name in ALWAYS_INCLUDE {
        let rel = PathBuf::from(name);
        if root.join(&rel).is_file() && seen.insert(rel.clone()) {
            files.push(rel);
        }
    }

    files.sort();
    Ok(files)
}

fn should_include(rel: &Path) -> bool {
    let components = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if components
        .iter()
        .any(|c| ALWAYS_EXCLUDE.contains(&c.as_str()))
    {
        return false;
    }
    true
}
