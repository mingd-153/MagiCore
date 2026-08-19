use super::*;

#[test]
fn test_get_embedded_template_returns_files_for_react_axum_fastapi() {
    let react = get_embedded_template("web", "react").unwrap();
    assert!(react.iter().any(|f| f.path == "package.json"));
    assert!(react.iter().any(|f| f.path == "vite.config.ts"));

    let axum = get_embedded_template("web", "axum").unwrap();
    assert!(axum.iter().any(|f| f.path == "Cargo.toml"));
    assert!(axum.iter().any(|f| f.path == "src/main.rs"));

    let fastapi = get_embedded_template("web", "fastapi").unwrap();
    assert!(fastapi.iter().any(|f| f.path == "main.py"));
    assert!(fastapi.iter().any(|f| f.path == "pyproject.toml"));
}

#[test]
fn test_materialize_embedded_performs_in_memory_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("my-fast-app");
    let files = get_embedded_template("web", "react").unwrap();

    let res = materialize_embedded(&root, "my-fast-app", &files);
    assert!(res.is_ok());

    let pkg_json = std::fs::read_to_string(root.join("package.json")).unwrap();
    assert!(pkg_json.contains(r#""name": "my-fast-app""#));

    let index_html = std::fs::read_to_string(root.join("index.html")).unwrap();
    assert!(index_html.contains("<title>my-fast-app</title>"));
}
