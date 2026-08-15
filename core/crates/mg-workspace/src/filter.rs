//! Filter — glob match tên package (`@core/*`) hoặc path tương đối (`./apps/*`).

/// Match 1 node có khớp filter không.
/// - `./apps/*` / `apps/*`: match path tương đối (globbing chỉ 1 cấp sau wildcard `*`)
/// - `@core/*`: match prefix tên scoped package
/// - tên đầy đủ: match chính xác
pub fn filter_matches(filter: &str, relative_path: &std::path::Path, name: &str) -> bool {
    // Scoped package prefix: @core/*
    if filter.starts_with('@') {
        if let Some(scope) = filter.strip_suffix("/*") {
            return name == scope || name.starts_with(&format!("{scope}/"));
        }
        return filter == name;
    }
    // Path-style: ./apps/* hoặc apps/*
    let path_part = filter.strip_prefix("./").unwrap_or(filter);
    if path_part.contains('/') {
        return glob_match_path(path_part, relative_path);
    }
    // Exact name
    name == filter
}

/// Glob đơn giản: `*` = 1 segment; `**` = nhiều segment.
fn glob_match_path(pattern: &str, path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    let path_parts: Vec<&str> = path_str.split('/').filter(|p| !p.is_empty()).collect();
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|p| !p.is_empty()).collect();

    match pattern_parts.as_slice() {
        [] => path_parts.is_empty(),
        [only] if *only == "*" => !path_parts.is_empty(),
        [only] if *only == "**" => true,
        _ => {
            if pattern_parts.contains(&"**") {
                return glob_double_star(&pattern_parts, &path_parts);
            }
            glob_single(&pattern_parts, &path_parts)
        }
    }
}

fn glob_single(pattern_parts: &[&str], path_parts: &[&str]) -> bool {
    if pattern_parts.len() != path_parts.len() {
        return false;
    }
    pattern_parts
        .iter()
        .zip(path_parts)
        .all(|(p, a)| segment_matches(p, a))
}

fn segment_matches(pattern: &str, actual: &str) -> bool {
    pattern == "*" || pattern == actual
}

fn glob_double_star(pattern_parts: &[&str], path_parts: &[&str]) -> bool {
    // Chia pattern quanh ** và match 2 đầu
    let (head, tail) = match pattern_parts.iter().position(|p| *p == "**") {
        Some(index) => (&pattern_parts[..index], &pattern_parts[index + 1..]),
        None => return glob_single(pattern_parts, path_parts),
    };
    if head.len() + tail.len() > path_parts.len() {
        return false;
    }
    glob_single(head, &path_parts[..head.len()])
        && glob_single(tail, &path_parts[path_parts.len() - tail.len()..])
}
