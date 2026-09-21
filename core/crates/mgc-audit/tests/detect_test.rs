//! `detect_test.rs` — R1: one detection rule per manifest, shared by
//! every adapter. Each constructor fires exactly on its manifest set
//! and labels the step with the contract ecosystem/scanner pair.
//! Một luật nhận diện cho mỗi manifest — constructor nổ đúng trên tập
//! manifest của nó, nhãn step đúng cặp ecosystem/scanner.

#![allow(clippy::unwrap_used)]

use mgc_audit::detect::{dotnet_step, go_step, java_step, python_step, rust_step};

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-detect-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn empty_project_yields_no_steps() {
    let dir = tmp("empty");
    assert!(rust_step(&dir).is_none());
    assert!(python_step(&dir).is_none());
    assert!(go_step(&dir).is_none());
    assert!(java_step(&dir).is_none());
    assert!(dotnet_step(&dir).is_none());
}

#[test]
fn rust_step_fires_on_cargo_toml() {
    let dir = tmp("rust");
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    let step = rust_step(&dir).expect("Cargo.toml must yield a rust step");
    assert_eq!(step.ecosystem, "rust");
    assert_eq!(step.scanner, "cargo-audit");
}

#[test]
fn python_step_fires_on_every_recognized_manifest() {
    for file in [
        "requirements.txt",
        "requirements-dev.txt",
        "pylock.toml",
        "pylock.dev.toml",
        "uv.lock",
        "pyproject.toml",
    ] {
        let dir = tmp(&format!("py-{}", file.replace('.', "-")));
        std::fs::write(dir.join(file), "").unwrap();
        let step = python_step(&dir).unwrap_or_else(|| panic!("{file} must yield a python step"));
        assert_eq!(step.ecosystem, "python");
        assert_eq!(step.scanner, "pip-audit");
    }
}

#[test]
fn go_step_fires_on_go_mod() {
    let dir = tmp("go");
    std::fs::write(dir.join("go.mod"), "module example.com/x\n").unwrap();
    let step = go_step(&dir).expect("go.mod must yield a go step");
    assert_eq!(step.ecosystem, "go");
    assert_eq!(step.scanner, "govulncheck");
}

#[test]
fn java_step_fires_on_either_gradle_shape() {
    for file in ["build.gradle", "build.gradle.kts"] {
        let dir = tmp(&format!("jv-{}", file.replace('.', "-")));
        std::fs::write(dir.join(file), "").unwrap();
        let step = java_step(&dir).unwrap_or_else(|| panic!("{file} must yield a java step"));
        assert_eq!(step.ecosystem, "java");
        assert_eq!(step.scanner, "osv-maven");
    }
}

#[test]
fn dotnet_step_fires_on_csproj() {
    let dir = tmp("dotnet");
    std::fs::write(dir.join("app.csproj"), "<Project/>\n").unwrap();
    let step = dotnet_step(&dir).expect("*.csproj must yield a dotnet step");
    assert_eq!(step.ecosystem, "dotnet");
    assert_eq!(step.scanner, "osv-nuget");
}

/// REVIEW F1: the swift locator scope — root, `ios/`, exactly one
/// parent. Anything farther is another scope's lockfile.
/// Scope locator swift — root, `ios/`, đúng một cấp cha.
#[test]
fn swift_resolved_path_scope_is_bounded() {
    use mgc_audit::scanners::swift_resolved_path;
    let lock = r#"{"pins":[],"version":2}"#;

    let root_hit = tmp("swift-root");
    std::fs::write(root_hit.join("Package.resolved"), lock).unwrap();
    assert_eq!(
        swift_resolved_path(&root_hit).as_deref(),
        Some(root_hit.join("Package.resolved").as_path())
    );

    let rn = tmp("swift-rn");
    std::fs::create_dir_all(rn.join("ios")).unwrap();
    std::fs::write(rn.join("ios").join("Package.resolved"), lock).unwrap();
    assert!(swift_resolved_path(&rn).is_some());

    let outer = tmp("swift-outer");
    std::fs::write(outer.join("Package.resolved"), lock).unwrap();
    let mid = outer.join("mid");
    std::fs::create_dir_all(&mid).unwrap();
    assert!(
        swift_resolved_path(&mid).is_some(),
        "one parent level is in scope"
    );
    let deep = mid.join("deep");
    std::fs::create_dir_all(&deep).unwrap();
    assert!(
        swift_resolved_path(&deep).is_none(),
        "two levels up is another scope"
    );

    let bare = tmp("swift-bare");
    assert!(swift_resolved_path(&bare).is_none());
}

/// P0/F4: Maven pom.xml triggers the java lane; a .sln triggers the
/// dotnet lane (nested projects resolve inside the scanner).
/// pom.xml kích hoạt lane java; .sln kích hoạt lane dotnet.
#[test]
fn java_step_fires_on_pom_xml() {
    let dir = tmp("jv-pom");
    std::fs::write(dir.join("pom.xml"), "<project/>\n").unwrap();
    let step = java_step(&dir).expect("pom.xml must yield a java step");
    assert_eq!(step.ecosystem, "java");
    assert_eq!(step.scanner, "osv-maven");
}

#[test]
fn dotnet_step_fires_on_solution() {
    let dir = tmp("dotnet-sln");
    std::fs::write(
        dir.join("app.sln"),
        "Microsoft Visual Studio Solution File\n",
    )
    .unwrap();
    let step = dotnet_step(&dir).expect("*.sln must yield a dotnet step");
    assert_eq!(step.ecosystem, "dotnet");
    assert_eq!(step.scanner, "osv-nuget");
}
