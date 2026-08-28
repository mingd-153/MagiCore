#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for CLI bundler

use super::*;
use tempfile::tempdir;

#[test]
fn prepare_workspace_links_node_modules_instead_of_mirroring_tree() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("node_modules")).unwrap();
    std::fs::create_dir_all(root.join("node_modules/.magicore")).unwrap();
    std::fs::create_dir_all(root.join(".magicore")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"demo","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.tsx"), "export const ok = true;").unwrap();

    let prepared = prepare_workspace(&root.join("src/main.tsx")).unwrap();

    assert_ne!(prepared.working_dir, root);
    assert!(prepared.entry.exists());
    let linked_node_modules = prepared.working_dir.join("node_modules");
    let metadata = std::fs::symlink_metadata(&linked_node_modules).unwrap();
    assert!(metadata.file_type().is_symlink() || metadata.is_dir());
}

#[test]
fn process_assets_rewrites_root_index_html_to_built_bundle() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("dist")).unwrap();
    std::fs::write(root.join("src/main.tsx"), "console.log('ok');").unwrap();
    std::fs::write(
        root.join("index.html"),
        "<!doctype html><html><body><div id=\"root\"></div><script type=\"module\" src=\"/src/main.tsx\"></script></body></html>",
    )
    .unwrap();

    let config = BundlerConfig {
        entry: root.join("src/main.tsx"),
        output_dir: root.join("dist"),
        minify: true,
        sourcemap: true,
        target: "es2020".to_string(),
        public_path: "/".to_string(),
    };

    materialize_index_html(root, &config).unwrap();

    let generated = std::fs::read_to_string(root.join("dist/index.html")).unwrap();
    assert!(generated.contains("src=\"/main.js\""));
    assert!(!generated.contains("/src/main.tsx"));
}
