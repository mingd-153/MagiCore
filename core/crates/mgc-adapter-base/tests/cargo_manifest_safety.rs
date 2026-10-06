#![allow(clippy::unwrap_used)]

use mgc_adapter_base::cargo_manifest::{parse_manifest, write_manifest};
use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};

fn dependency(name: &str, version: &str) -> DependencySpec {
    DependencySpec::new(
        PackageName::new(name).unwrap(),
        VersionRange::parse(version).unwrap(),
    )
}

#[test]
fn native_writer_preserves_cargo_specific_and_non_root_dependency_tables() {
    let project = tempfile::tempdir().unwrap();
    let original = r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"], default-features = false }
removed-by-user = "0.1"

[dev-dependencies]
tokio = { version = "1.0", features = ["macros"] }

[build-dependencies]
cc = "1.0"

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[features]
default = []
"#;
    std::fs::write(project.path().join("Cargo.toml"), original).unwrap();

    let mut manifest = Manifest::new("demo", Ecosystem::Lib);
    manifest.add_dep(dependency("serde", "1.2.3"), false, false, false);
    manifest.add_dep(dependency("anyhow", "1.0.0"), false, false, false);
    manifest.add_dep(dependency("tokio", "1.0.0"), true, false, false);
    write_manifest(project.path(), &manifest).unwrap();

    let body = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&body).unwrap();
    let dependencies = parsed["dependencies"].as_table().unwrap();
    let serde = dependencies["serde"].as_table().unwrap();
    assert_eq!(serde["version"].as_str(), Some("1.2.3"));
    assert_eq!(serde["features"][0].as_str(), Some("derive"));
    assert_eq!(serde["default-features"].as_bool(), Some(false));
    assert!(dependencies.contains_key("anyhow"));
    assert!(!dependencies.contains_key("removed-by-user"));
    assert_eq!(
        parsed["dev-dependencies"]["tokio"]["features"][0].as_str(),
        Some("macros")
    );
    assert!(parsed["build-dependencies"]["cc"].is_str());
    assert!(parsed["target"]["cfg(unix)"]["dependencies"]["libc"].is_str());
    assert!(parsed["features"]["default"].is_array());
}

#[test]
fn native_resolver_rejects_manifest_semantics_it_cannot_represent() {
    let cases = [
        (
            "path dependency",
            "[dependencies]\nlocal = { path = \"../local\" }\n",
        ),
        ("build dependency", "[build-dependencies]\ncc = \"1\"\n"),
        (
            "target dependency",
            "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        ),
        (
            "feature dependency",
            "[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
        ),
    ];

    for (label, body) in cases {
        let project = tempfile::tempdir().unwrap();
        let cargo_toml =
            format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{body}");
        std::fs::write(project.path().join("Cargo.toml"), cargo_toml).unwrap();
        assert!(
            parse_manifest(project.path(), Ecosystem::Lib).is_err(),
            "{label} must not be silently omitted from the native graph"
        );
    }
}
