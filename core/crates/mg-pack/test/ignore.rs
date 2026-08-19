//! Integration tests for ignore parser — test riêng tại test/ (RULE §5)
use mg_pack::ignore::select_files;
use std::fs;
use std::io::Write;

fn write(root: &std::path::Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = fs::File::create(p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn respects_npmignore_preferring_it_over_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "package.json", "{}");
    write(root, "src/index.js", "export {}");
    write(root, "src/secret.js", "secrets");
    write(root, ".gitignore", "node_modules\n");
    write(root, ".npmignore", "src/secret.js\n");

    let files = select_files(root).unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"src/index.js".to_string()));
    assert!(!names.contains(&"src/secret.js".to_string()));
    assert!(names.contains(&"package.json".to_string()));
}

#[test]
fn excludes_always_excluded_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "package.json", "{}");
    write(root, "node_modules/lodash/index.js", "x");
    write(root, ".git/config", "x");

    let files = select_files(root).unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(!names.iter().any(|n| n.starts_with("node_modules")));
    assert!(!names.iter().any(|n| n.starts_with(".git")));
    assert!(names.contains(&"package.json".to_string()));
}

#[test]
fn always_includes_readme_license_changelog_even_if_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "package.json", "{}");
    write(root, "README.md", "readme");
    write(root, "LICENSE", "mit");
    write(root, ".gitignore", "README.md\nLICENSE\n");

    let files = select_files(root).unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"README.md".to_string()));
    assert!(names.contains(&"LICENSE".to_string()));
}

#[test]
fn falls_back_to_gitignore_without_npmignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "package.json", "{}");
    write(root, "src/index.js", "x");
    write(root, "src/secret.js", "x");
    write(root, ".gitignore", "src/secret.js\n");

    let files = select_files(root).unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"src/index.js".to_string()));
    assert!(!names.contains(&"src/secret.js".to_string()));
}

#[test]
fn returns_sorted_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "package.json", "{}");
    write(root, "b.js", "x");
    write(root, "a.js", "x");

    let files = select_files(root).unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}
