//! `manifest_test.rs` — go.mod read-only parser tests (P1 Go lane).
//! Block require, single-line require, `// indirect` comments — versions
//! must stay clean of comment text. Since the Phase 2 native Go engine the
//! manifest keeps FULL module paths (PackageName repo-path form) and
//! applies the project's own `replace`/`exclude` directives — the
//! effective module graph.
//! Test parser chỉ-đọc go.mod: block require, require một dòng, comment
//! `// indirect` — version không dính văn bản comment. Từ engine Go native
//! Phase 2, manifest giữ NGUYÊN path module (dạng repo-path của
//! PackageName) và áp directive `replace`/`exclude` của project — graph
//! module hiệu lực.

#![allow(clippy::unwrap_used)]

use crate::manifest::parse_go_mod_manifest;
use crate::manifest::{
    parse_cargo_manifest, parse_pyproject_manifest, supports_native_python_project,
    write_cargo_manifest, write_pyproject_manifest,
};

#[test]
fn native_python_ownership_accepts_only_supported_pep621_inputs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"demo\"\ndependencies = [\"requests>=2\"]\n",
    )
    .unwrap();
    assert!(supports_native_python_project(dir.path()));

    for lock in [
        "uv.lock",
        "PoEtRy.LoCk",
        "pdm.lock",
        "Pipfile.lock",
        "requirements-dev.txt",
        "pylock.3.13.toml",
    ] {
        std::fs::write(dir.path().join(lock), "foreign lock").unwrap();
        assert!(!supports_native_python_project(dir.path()), "{lock}");
        std::fs::remove_file(dir.path().join(lock)).unwrap();
    }

    for source in [
        "[project]\nname=\"demo\"\noptional-dependencies={test=[\"pytest\"]}\n",
        "[project]\nname=\"demo\"\ndynamic=[\"dependencies\"]\n",
        "[project]\nname=\"demo\"\n[tool.poetry.dependencies]\nrequests=\"^2\"\n",
        "[project]\nname=\"demo\"\n[tool.uv.sources]\nrequests={git=\"https://example.invalid/r.git\"}\n",
        "[project]\nname=\"demo\"\n[dependency-groups]\ntest=[\"pytest\"]\n",
    ] {
        std::fs::write(dir.path().join("pyproject.toml"), source).unwrap();
        assert!(!supports_native_python_project(dir.path()), "{source}");
    }
}

fn write_go_mod(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join("go.mod"), content).unwrap();
}

#[test]
fn single_line_require_with_indirect_comment_keeps_clean_version() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        "module example.com/m\n\ngo 1.26\n\nrequire golang.org/x/text v0.3.2 // indirect\n",
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    // Full module path is the canonical dep name (native engine resolves
    // through it).
    // (Path module đầy đủ là tên dep canonical — engine native resolve qua
    // nó.)
    let dep = manifest
        .find_dep("golang.org/x/text")
        .expect("full module path is the dep name");
    // The version range must parse `0.3.2` — NOT `v0.3.2 // indirect`.
    // Range version phải parse `0.3.2` — KHÔNG PHẢI `v0.3.2 // indirect`.
    assert_eq!(
        dep.range.satisfying_version().map(|v| v.to_string()),
        Some("0.3.2".to_string()),
        "comment must not contaminate the version"
    );
}

#[test]
fn block_require_parses_every_entry_and_module_name() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/poly\n\n",
            "go 1.26\n\n",
            "require (\n",
            "\tgolang.org/x/text v0.3.2 // indirect\n",
            "\tgopkg.in/yaml.v3 v3.0.1\n",
            ")\n\n",
            "require github.com/sergi/go-diff v1.1.0\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    assert_eq!(manifest.name, "example.com/poly");
    assert!(manifest.find_dep("golang.org/x/text").is_some());
    assert!(manifest.find_dep("gopkg.in/yaml.v3").is_some());
    assert!(manifest.find_dep("github.com/sergi/go-diff").is_some());
}

#[test]
fn exclude_drops_the_pinned_require() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/ex\n\n",
            "go 1.26\n\n",
            "require (\n",
            "\tgolang.org/x/text v0.3.2\n",
            "\tgolang.org/x/bad v0.1.0\n",
            ")\n\n",
            "exclude golang.org/x/bad v0.1.0\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("golang.org/x/text").is_some());
    assert!(
        manifest.find_dep("golang.org/x/bad").is_none(),
        "excluded (module, version) pin is dropped from the graph"
    );
}

#[test]
fn replace_rewrites_the_required_pin() {
    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        concat!(
            "module example.com/rp\n\n",
            "go 1.26\n\n",
            "require (\n",
            "\tgolang.org/x/old v1.2.0\n",
            "\texample.com/other v0.1.0\n",
            ")\n\n",
            "replace golang.org/x/old => github.com/fork/old v1.2.1\n",
            // Version-scoped replace that does NOT match the pin — no-op.
            // (Replace theo version KHÔNG khớp pin — no-op.)
            "replace example.com/other v9.9.9 => example.com/never v0.0.1\n",
        ),
    );
    let manifest = parse_go_mod_manifest(dir.path()).unwrap();
    let replaced = manifest
        .find_dep("github.com/fork/old")
        .expect("replace rewrote the pin");
    assert_eq!(
        replaced.range.satisfying_version().map(|v| v.to_string()),
        Some("1.2.1".to_string())
    );
    assert!(manifest.find_dep("golang.org/x/old").is_none());
    // Unmatched (version-scoped) replace leaves the pin untouched.
    // (Replace (theo version) không khớp giữ nguyên pin.)
    let untouched = manifest.find_dep("example.com/other").unwrap();
    assert_eq!(
        untouched.range.satisfying_version().map(|v| v.to_string()),
        Some("0.1.0".to_string())
    );
    assert!(manifest.find_dep("example.com/never").is_none());
}

/// pyproject round-trip (C0 FIX2): star deps serialize as the bare PEP 508
/// name (never dropped), `==` pins stay exact (never widened to `>=`),
/// `>=` bounds survive untouched. A hand-written `["six"]` must survive
/// parse → write → parse — dropping it faked a manifest mutation.
/// (Round-trip pyproject: `*` thành tên trần, `==` giữ exact.)
#[test]
fn pyproject_star_and_pins_round_trip_losslessly() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"rt\"\ndependencies = [\"six\", \"attrs==23.1.0\", \"req>=2.0\"]\n",
    )
    .unwrap();
    let manifest = parse_pyproject_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("six").unwrap().range.is_star());
    write_pyproject_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("pyproject.toml")).unwrap();
    assert!(
        body.contains("\"six\""),
        "star must persist as bare name: {body}"
    );
    assert!(
        body.contains("attrs==23.1.0"),
        "exact pin must stay ==: {body}"
    );
    assert!(
        body.contains("req>=2.0"),
        "lower bound must survive: {body}"
    );
    let again = parse_pyproject_manifest(dir.path()).unwrap();
    assert!(again.find_dep("six").is_some());
    assert!(again.find_dep("attrs").is_some());
    assert!(again.find_dep("req").is_some());
}

/// Cargo round-trip (C0 FIX2): `*` is valid Cargo — it serializes back as
/// `*` instead of vanishing on the next add.
/// (Round-trip Cargo: `*` giữ lại, không mất.)
#[test]
fn cargo_star_round_trips_instead_of_vanishing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"rt\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"*\"\n",
    )
    .unwrap();
    let manifest = parse_cargo_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("serde").unwrap().range.is_star());
    write_cargo_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(body.contains("serde"), "star dep must persist: {body}");
    let again = parse_cargo_manifest(dir.path()).unwrap();
    assert!(again.find_dep("serde").is_some(), "star dep must re-parse");
}

/// go.mod writer (native add): mgc owns require pins — module/go/replace/
/// exclude lines survive, requires come from the manifest, versions always
/// v-prefixed. Round-trip parse → write → parse is stable.
/// (Writer go.mod: giữ directive, require từ manifest.)
#[test]
fn go_mod_writer_pins_require_and_round_trips() {
    use crate::manifest::write_go_mod_manifest;
    use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};

    let dir = tempfile::tempdir().unwrap();
    write_go_mod(
        dir.path(),
        "module example.com/m\n\ngo 1.21\n\nreplace example.com/old => example.com/new v1.0.0\n",
    );
    let mut manifest = Manifest::new("m", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("github.com/google/uuid").unwrap(),
            VersionRange::parse("1.6.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    write_go_mod_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("go.mod")).unwrap();
    assert!(
        body.contains("github.com/google/uuid v1.6.0"),
        "require pin must be written v-prefixed: {body}"
    );
    assert!(
        body.contains("module example.com/m") && body.contains("replace example.com/old"),
        "non-require directives must survive: {body}"
    );
    let again = parse_go_mod_manifest(dir.path()).unwrap();
    let dep = again
        .find_dep("github.com/google/uuid")
        .expect("re-parse finds uuid");
    assert_eq!(
        dep.range.satisfying_version().map(|v| v.to_string()),
        Some("1.6.0".to_string())
    );
}

/// csproj writer (native add): mgc owns PackageReference pins — inserts
/// `<PackageReference Include Version>` into an ItemGroup (creating one
/// when absent), preserving everything else. Round-trip stable.
/// (Writer csproj: ghim PackageReference, giữ phần còn lại.)
#[test]
fn csproj_writer_pins_reference_and_round_trips() {
    use crate::manifest::{parse_csproj_manifest, write_csproj_manifest};
    use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("demo.csproj"),
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n  </PropertyGroup>\n</Project>\n",
    )
    .unwrap();
    let mut manifest = Manifest::new("demo", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("Newtonsoft.Json").unwrap(),
            VersionRange::parse("13.0.3").unwrap(),
        ),
        false,
        false,
        false,
    );
    write_csproj_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("demo.csproj")).unwrap();
    assert!(
        body.contains("Newtonsoft.Json") && body.contains("13.0.3"),
        "PackageReference pin must be written: {body}"
    );
    assert!(
        body.contains("<TargetFramework>net8.0</TargetFramework>"),
        "project body must survive: {body}"
    );
    let again = parse_csproj_manifest(dir.path()).unwrap();
    let dep = again
        .find_dep("Newtonsoft.Json")
        .expect("re-parse finds pin");
    assert_eq!(
        dep.range.satisfying_version().map(|v| v.to_string()),
        Some("13.0.3".to_string()),
        "re-parsed pin must hold the version"
    );
}

/// pom.xml writer (native add): mgc owns dependency pins — inserts a
/// `<dependency>` block into top-level `<dependencies>` (creating the
/// block when absent), preserving everything else. Round-trip stable.
/// (Writer pom.xml: ghim dependency, giữ phần còn lại.)
#[test]
fn pom_writer_pins_dependency_and_round_trips() {
    use crate::manifest::{parse_maven_manifest, write_pom_manifest};
    use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pom.xml"),
        "<project>\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.example</groupId>\n  <artifactId>demo</artifactId>\n  <version>0.1.0</version>\n  <dependencyManagement>\n    <dependencies>\n      <dependency>\n        <groupId>org.managed</groupId>\n        <artifactId>managed-lib</artifactId>\n        <version>9.9.9</version>\n      </dependency>\n    </dependencies>\n  </dependencyManagement>\n</project>\n",
    )
    .unwrap();
    let mut manifest = Manifest::new("demo", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("org.apache.commons:commons-lang3").unwrap(),
            VersionRange::parse("3.14.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    write_pom_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("pom.xml")).unwrap();
    assert!(
        body.contains("commons-lang3") && body.contains("3.14.0"),
        "dependency pin must be written: {body}"
    );
    assert!(
        body.contains("<artifactId>demo</artifactId>"),
        "project body must survive: {body}"
    );
    assert!(
        body.contains("<artifactId>managed-lib</artifactId>"),
        "dependencyManagement must survive untouched: {body}"
    );
    let managed_count = body.matches("managed-lib").count();
    assert_eq!(managed_count, 1, "managed entry must not duplicate: {body}");
    let again = parse_maven_manifest(dir.path()).unwrap();
    let dep = again
        .find_dep("org.apache.commons:commons-lang3")
        .expect("re-parse finds pin");
    assert_eq!(
        dep.range.satisfying_version().map(|v| v.to_string()),
        Some("3.14.0".to_string()),
        "re-parsed pin must hold the version"
    );
}

/// csproj remove honesty: pins absent from the manifest are deleted,
/// not left behind.
#[test]
fn csproj_writer_prunes_removed_references() {
    use crate::manifest::write_csproj_manifest;
    use mgc_types::{Ecosystem, Manifest};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("demo.csproj"),
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <ItemGroup>\n    <PackageReference Include=\"Old.Lib\" Version=\"1.0.0\" />\n  </ItemGroup>\n</Project>\n",
    )
    .unwrap();
    let manifest = Manifest::new("demo", Ecosystem::Lib);
    write_csproj_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("demo.csproj")).unwrap();
    assert!(
        !body.contains("Old.Lib"),
        "removed reference must be gone: {body}"
    );
}

/// pom remove honesty: stale top-level pins are deleted, managed ones kept.
#[test]
fn pom_writer_prunes_removed_dependencies() {
    use crate::manifest::write_pom_manifest;
    use mgc_types::{Ecosystem, Manifest};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pom.xml"),
        "<project>\n  <modelVersion>4.0.0</modelVersion>\n  <dependencies>\n    <dependency>\n      <groupId>com.old</groupId>\n      <artifactId>old-lib</artifactId>\n      <version>1.0.0</version>\n    </dependency>\n  </dependencies>\n</project>\n",
    )
    .unwrap();
    let manifest = Manifest::new("demo", Ecosystem::Lib);
    write_pom_manifest(dir.path(), &manifest).unwrap();
    let body = std::fs::read_to_string(dir.path().join("pom.xml")).unwrap();
    assert!(
        !body.contains("old-lib"),
        "removed dependency must be gone: {body}"
    );
}
