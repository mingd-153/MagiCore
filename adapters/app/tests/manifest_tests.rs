#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Manifest parsing tests for app adapter.

use mgc_app_adapter::AppLanguage;
use mgc_app_adapter::manifest::parse_manifest;

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-app-manifest-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[test]
fn parse_flutter_pubspec_with_dependencies() {
    let dir = tmp("flutter");
    std::fs::write(
        dir.join("pubspec.yaml"),
        "name: myapp\ndependencies:\n  http: ^1.0.0\n  provider: ^6.0.0\n",
    )
    .unwrap();

    let manifest = parse_manifest(AppLanguage::Flutter, &dir).unwrap();
    assert_eq!(manifest.name, "myapp");
    assert!(manifest.find_dep("http").is_some());
    assert!(manifest.find_dep("provider").is_some());
}

#[test]
fn parse_flutter_pubspec_empty_deps() {
    let dir = tmp("flutter-empty");
    std::fs::write(dir.join("pubspec.yaml"), "name: empty\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Flutter, &dir).unwrap();
    assert_eq!(manifest.name, "empty");
}

#[test]
fn parse_kotlin_gradle_returns_manifest() {
    let dir = tmp("kotlin");
    std::fs::write(dir.join("build.gradle"), "// gradle build\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Kotlin, &dir).unwrap();
    assert!(!manifest.name.is_empty());
}

#[test]
fn parse_swift_package_fails_closed_on_invalid_manifest() {
    let dir = tmp("swift");
    // Phase 2 native lane: Package.swift is Swift SOURCE — only the
    // toolchain can evaluate it. A placeholder/broken manifest fails
    // closed (with or without a toolchain installed) — never an empty
    // false-success manifest.
    // (Lane native Phase 2: Package.swift là MÃ NGUỒN Swift — chỉ toolchain
    // đánh giá được. Manifest placeholder/hỏng fail-closed (có hoặc không
    // có toolchain) — không bao giờ manifest rỗng thành công giả.)
    std::fs::write(dir.join("Package.swift"), "// swift package\n").unwrap();
    let err = parse_manifest(AppLanguage::Swift, &dir).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("dump-package") || msg.contains("Swift toolchain"),
        "fail-closed guidance required: {msg}"
    );
}

#[test]
fn parse_react_native_package_json() {
    let dir = tmp("rn");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"myapp","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();

    let manifest = parse_manifest(AppLanguage::ReactNative, &dir).unwrap();
    assert_eq!(manifest.name, "myapp");
}

#[test]
fn parse_multi_detects_flutter() {
    let dir = tmp("multi-flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: multiapp\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Multi, &dir).unwrap();
    assert_eq!(manifest.name, "multiapp");
}

/// pubspec writer round-trip: pins written v-implied-^, SDK + project
/// keys preserved, dev deps kept separate, re-parse stable.
#[test]
fn write_pubspec_pins_and_round_trips() {
    use mgc_app_adapter::manifest::{parse_manifest, write_manifest};
    use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};

    let dir = tmp("flutter-write");
    std::fs::write(
        dir.join("pubspec.yaml"),
        "name: myapp\ndescription: keep me\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    let mut manifest = Manifest::new("myapp", Ecosystem::App);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("meta").unwrap(),
            VersionRange::parse("1.12.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    write_manifest(AppLanguage::Flutter, &dir, &manifest).unwrap();
    let body = std::fs::read_to_string(dir.join("pubspec.yaml")).unwrap();
    assert!(
        body.contains("meta") && body.contains("1.12.0"),
        "pin written: {body}"
    );
    assert!(
        body.contains("description: keep me"),
        "other keys survive: {body}"
    );
    assert!(
        body.contains("sdk: flutter"),
        "SDK pseudo-dep survives: {body}"
    );
    let again = parse_manifest(AppLanguage::Flutter, &dir).unwrap();
    let dep = again.find_dep("meta").expect("re-parse finds meta");
    assert_eq!(
        dep.range.satisfying_version().map(|v| v.to_string()),
        Some("1.12.0".to_string())
    );
}

/// Version-catalog round-trip: ref + inline pins parse, bump rewrites,
/// re-parse verifies. No toolchain anywhere (pure TOML).
#[test]
fn catalog_pins_parse_and_bump() {
    use mgc_app_adapter::manifest::gradle::{bump_catalog_pin, parse_version_catalog};

    let dir = tmp("gradle-catalog");
    std::fs::create_dir_all(dir.join("gradle")).unwrap();
    std::fs::write(
        dir.join("gradle/libs.versions.toml"),
        "[versions]\nlang3 = \"3.12.0\"\n\n[libraries]\ncommons-lang3 = { module = \"org.apache.commons:commons-lang3\", version.ref = \"lang3\" }\nguava = \"com.google.guava:guava:32.0.0\"\n",
    )
    .unwrap();
    let pins = parse_version_catalog(&dir).expect("catalog parses");
    assert_eq!(pins.len(), 2);
    let lang3 = pins.iter().find(|p| p.artifact == "commons-lang3").unwrap();
    assert_eq!(lang3.version, "3.12.0");
    assert_eq!(lang3.version_ref.as_deref(), Some("lang3"));

    assert!(bump_catalog_pin(&dir, "org.apache.commons", "commons-lang3", "3.14.0").unwrap());
    let body = std::fs::read_to_string(dir.join("gradle/libs.versions.toml")).unwrap();
    assert!(body.contains("3.14.0"), "ref value bumped:\n{body}");
    assert!(!body.contains("3.12.0"), "old pin gone:\n{body}");

    assert!(bump_catalog_pin(&dir, "com.google.guava", "guava", "33.0.0").unwrap());
    let pins = parse_version_catalog(&dir).expect("re-parse works");
    let guava = pins.iter().find(|p| p.artifact == "guava").unwrap();
    assert_eq!(guava.version, "33.0.0");

    // Unknown dep: honest false, never a guess.
    assert!(!bump_catalog_pin(&dir, "no.such", "lib", "1.0.0").unwrap());
    // Missing catalog: None, not an error.
    let empty = tempfile::tempdir().unwrap();
    assert!(parse_version_catalog(empty.path()).is_none());
}
