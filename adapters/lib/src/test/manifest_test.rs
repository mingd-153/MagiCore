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

use crate::language::{LibLanguage, dependency_manifest_format};
use crate::manifest::parse_go_mod_manifest;
use crate::manifest::{
    parse_cargo_manifest, parse_pyproject_manifest, supports_native_python_project,
    write_cargo_manifest, write_pyproject_manifest,
};
use mgc_types::capabilities::DependencyResolver;

#[test]
fn native_manifest_format_is_reported_only_for_owned_files() {
    let root = tempfile::tempdir().unwrap();
    for (language, file, contents, expected) in [
        (LibLanguage::Ts, "package.json", "{}\n", "package-json"),
        (
            LibLanguage::Rust,
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.1.0'\n",
            "cargo-toml",
        ),
        (
            LibLanguage::Go,
            "go.mod",
            "module example.test/fixture\n\ngo 1.24\n",
            "go-mod",
        ),
    ] {
        assert_eq!(dependency_manifest_format(root.path(), language), None);
        std::fs::write(root.path().join(file), contents).unwrap();
        assert_eq!(
            dependency_manifest_format(root.path(), language),
            Some(expected)
        );
        std::fs::remove_file(root.path().join(file)).unwrap();
    }

    std::fs::write(
        root.path().join("pyproject.toml"),
        "[project]\nname='fixture'\nversion='0.1.0'\ndependencies=[]\n",
    )
    .unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::Python),
        Some("pep621-native")
    );
    std::fs::write(root.path().join("uv.lock"), "version = 1\n").unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::Python),
        Some("pyproject-unsupported"),
        "foreign lock ownership must not be advertised as native"
    );
}

#[test]
fn java_native_format_is_maven_only_and_gradle_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::Java),
        None
    );

    std::fs::write(root.path().join("build.gradle.kts"), "plugins {}").unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::Java),
        Some("gradle")
    );

    std::fs::write(root.path().join("pom.xml"), "<project/>").unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::Java),
        Some("ambiguous"),
        "a mixed Maven/Gradle root must not silently select Maven"
    );
}

#[test]
fn java_native_format_selects_maven_when_gradle_is_absent() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("pom.xml"), "<project/>").unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::Java),
        Some("maven-pom")
    );
}

#[test]
fn java_adapter_fails_closed_when_maven_and_gradle_are_both_present() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("pom.xml"), "<project/>").unwrap();
    std::fs::write(root.path().join("build.gradle"), "plugins {}").unwrap();
    let adapter = crate::adapter_for_language(LibLanguage::Java, root.path(), None, None).unwrap();

    let error = adapter.probe_dependency_resolver().unwrap_err();

    assert!(error.to_string().contains("both Maven and Gradle"));
}

#[test]
fn maven_manifest_rejects_unresolved_direct_dependencies_instead_of_dropping_them() {
    use crate::manifest::parse_maven_manifest;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("pom.xml"),
        r#"<project>
  <groupId>com.example</groupId><artifactId>spring-app</artifactId><version>1.0.0</version>
  <dependencies><dependency>
    <groupId>org.springframework</groupId><artifactId>spring-core</artifactId>
  </dependency></dependencies>
</project>"#,
    )
    .unwrap();

    let error = parse_maven_manifest(root.path()).unwrap_err();

    assert!(error.to_string().contains("spring-core"));
    assert!(error.to_string().contains("cannot be resolved"));
}

#[cfg(unix)]
#[test]
fn native_cargo_go_and_python_writers_refuse_symlink_manifests() {
    use crate::manifest::{write_cargo_manifest, write_go_mod_manifest, write_pyproject_manifest};
    use std::os::unix::fs::symlink;

    for (manifest_name, writer) in [
        (
            "Cargo.toml",
            write_cargo_manifest
                as fn(&std::path::Path, &mgc_types::Manifest) -> mgc_types::MgResult<()>,
        ),
        ("go.mod", write_go_mod_manifest),
        ("pyproject.toml", write_pyproject_manifest),
    ] {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let external_manifest = outside.path().join("manifest");
        std::fs::write(&external_manifest, "untouched = true\n").unwrap();
        symlink(&external_manifest, project.path().join(manifest_name)).unwrap();

        let error = writer(
            project.path(),
            &mgc_types::Manifest::new("demo", mgc_types::Ecosystem::Lib),
        )
        .unwrap_err();

        assert!(error.to_string().contains("link") || error.to_string().contains("regular"));
        assert_eq!(
            std::fs::read_to_string(external_manifest).unwrap(),
            "untouched = true\n"
        );
    }
}

#[test]
fn maven_manifest_resolves_local_properties_and_literal_dependency_management() {
    use crate::manifest::parse_maven_manifest;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("pom.xml"),
        r#"<project>
  <groupId>com.example</groupId><artifactId>spring-app</artifactId><version>1.0.0</version>
  <properties><spring.version>6.1.0</spring.version></properties>
  <dependencyManagement><dependencies><dependency>
    <groupId>org.springframework</groupId><artifactId>spring-core</artifactId><version>6.1.1</version>
  </dependency></dependencies></dependencyManagement>
  <dependencies>
    <dependency><groupId>org.springframework</groupId><artifactId>spring-web</artifactId><version>${spring.version}</version></dependency>
    <dependency><groupId>org.springframework</groupId><artifactId>spring-core</artifactId></dependency>
  </dependencies>
</project>"#,
    )
    .unwrap();

    let manifest = parse_maven_manifest(root.path()).unwrap();

    assert_eq!(
        manifest
            .find_dep("org.springframework:spring-web")
            .unwrap()
            .range
            .to_string(),
        "6.1.0"
    );
    assert_eq!(
        manifest
            .find_dep("org.springframework:spring-core")
            .unwrap()
            .range
            .to_string(),
        "6.1.1"
    );
}

#[test]
fn maven_manifest_does_not_treat_commented_dependency_as_real() {
    use crate::manifest::parse_maven_manifest;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("pom.xml"),
        r#"<project>
  <groupId>com.example</groupId><artifactId>sample</artifactId><version>1.0.0</version>
  <!-- Example only:
  <dependencies><dependency><groupId>bad.example</groupId><artifactId>not-real</artifactId><version>9.9.9</version></dependency></dependencies>
  -->
  <dependencies><dependency><groupId>good.example</groupId><artifactId>real</artifactId><version>1.2.3</version></dependency></dependencies>
</project>"#,
    )
    .unwrap();

    let manifest = parse_maven_manifest(root.path()).unwrap();

    assert!(manifest.find_dep("good.example:real").is_some());
    assert!(manifest.find_dep("bad.example:not-real").is_none());
}

#[test]
fn maven_manifest_rejects_malformed_xml_instead_of_returning_an_empty_graph() {
    use crate::manifest::parse_maven_manifest;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("pom.xml"),
        "<project><dependencies><dependency><groupId>g</groupId></project>",
    )
    .unwrap();

    let error = parse_maven_manifest(root.path()).unwrap_err();

    assert!(error.to_string().contains("invalid Maven POM XML"));
}

#[cfg(unix)]
#[test]
fn maven_manifest_refuses_symlinked_pom_outside_project() {
    use crate::manifest::parse_maven_manifest;
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let external_pom = external.path().join("pom.xml");
    std::fs::write(
        &external_pom,
        "<project><dependencies><dependency><groupId>outside</groupId><artifactId>secret</artifactId><version>1.0.0</version></dependency></dependencies></project>",
    )
    .unwrap();
    symlink(&external_pom, project.path().join("pom.xml")).unwrap();

    let error = parse_maven_manifest(project.path()).unwrap_err();

    assert!(error.to_string().contains("symlink"));
}

#[test]
fn maven_manifest_reads_namespace_prefixed_project_elements() {
    use crate::manifest::parse_maven_manifest;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("pom.xml"),
        r#"<m:project xmlns:m="http://maven.apache.org/POM/4.0.0">
  <m:groupId>com.example</m:groupId><m:artifactId>sample</m:artifactId><m:version>1.0.0</m:version>
  <m:dependencies><m:dependency><m:groupId>org.example</m:groupId><m:artifactId>real</m:artifactId><m:version>1.2.3</m:version></m:dependency></m:dependencies>
</m:project>"#,
    )
    .unwrap();

    let manifest = parse_maven_manifest(root.path()).unwrap();

    assert!(manifest.find_dep("org.example:real").is_some());
}

#[test]
fn dotnet_native_format_requires_a_root_csproj_case_insensitively() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::DotNet),
        None
    );
    std::fs::write(root.path().join("Sample.CSPROJ"), "<Project/>").unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::DotNet),
        Some("csproj")
    );
    std::fs::write(root.path().join("Second.csproj"), "<Project/>").unwrap();
    assert_eq!(
        dependency_manifest_format(root.path(), LibLanguage::DotNet),
        None,
        "multiple root projects require an explicit selector"
    );
}

#[test]
fn dotnet_target_framework_uses_the_same_case_insensitive_unique_csproj_gate() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Sample.CSPROJ"),
        "<Project><PropertyGroup><TargetFramework>net9.0</TargetFramework></PropertyGroup></Project>",
    )
    .unwrap();
    assert_eq!(
        crate::adapter::read_dotnet_target_framework(root.path())
            .unwrap()
            .as_deref(),
        Some("net9.0")
    );

    std::fs::write(root.path().join("Other.csproj"), "<Project/>").unwrap();
    assert_eq!(
        crate::adapter::read_dotnet_target_framework(root.path()).unwrap(),
        None,
        "multiple project files must not make TFM selection diverge from ownership"
    );
}

#[cfg(unix)]
#[test]
fn dotnet_target_framework_refuses_symlinked_csproj() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external_csproj = outside.path().join("External.csproj");
    std::fs::write(
        &external_csproj,
        "<Project><PropertyGroup><TargetFramework>net9.0</TargetFramework></PropertyGroup></Project>",
    )
    .unwrap();
    symlink(&external_csproj, root.path().join("External.csproj")).unwrap();

    assert!(
        crate::adapter::read_dotnet_target_framework_file(&root.path().join("External.csproj"))
            .is_err(),
        "target framework detection must not read a symlink outside the project"
    );
}

#[test]
fn dotnet_manifest_refuses_multiple_root_csprojs_instead_of_empty_success() {
    use crate::manifest::parse_csproj_manifest;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("App.csproj"), "<Project/>").unwrap();
    std::fs::write(root.path().join("Tests.csproj"), "<Project/>").unwrap();

    let error = parse_csproj_manifest(root.path()).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("no unique regular root-level .csproj")
    );
}

#[test]
fn dotnet_manifest_refuses_unresolved_package_references_before_mutation() {
    use crate::manifest::{parse_csproj_manifest, write_csproj_manifest};
    use mgc_types::{Ecosystem, Manifest};

    for unresolved in [
        r#"<PackageReference Include="Floating" Version="1.*" />"#,
        r#"<PackageReference Include="CentralManaged" />"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        let original = format!(
            "<Project><ItemGroup><PackageReference Include=\"Pinned\" Version=\"1.2.3\" />{unresolved}</ItemGroup></Project>"
        );
        std::fs::write(root.path().join("App.csproj"), &original).unwrap();

        let error = parse_csproj_manifest(root.path())
            .expect_err("unresolved PackageReference must not be omitted from native graph");
        assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
        assert!(write_csproj_manifest(root.path(), &Manifest::new("app", Ecosystem::Lib)).is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join("App.csproj")).unwrap(),
            original,
            "refusal must leave every PackageReference unchanged"
        );
    }
}

#[test]
fn dotnet_manifest_refuses_unmodeled_package_reference_semantics_before_mutation() {
    use crate::manifest::{parse_csproj_manifest, write_csproj_manifest};
    use mgc_types::{Ecosystem, Manifest};

    for conditional_group in [
        r#"<ItemGroup Condition="'$(TargetFramework)' == 'net8.0'"><PackageReference Include="Conditional" Version="1.2.3" /></ItemGroup>"#,
        r#"<ItemGroup><PackageReference Include="Conditional" Version="1.2.3" Condition="'$(Configuration)' == 'Release'" /></ItemGroup>"#,
        r#"<ItemGroup><PackageReference Update="Pinned" Version="2.0.0" /></ItemGroup>"#,
        r#"<ItemGroup><PackageReference Remove="Pinned" /></ItemGroup>"#,
        r#"<m:ItemGroup xmlns:m="urn:msbuild"><m:PackageReference Include="Prefixed" Version="1.2.3" /></m:ItemGroup>"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        let original = format!(
            "<Project><!-- <ItemGroup Condition=\"comment only\"> --><ItemGroup><PackageReference Include=\"Pinned\" Version=\"1.2.3\" /></ItemGroup>{conditional_group}</Project>"
        );
        std::fs::write(root.path().join("App.csproj"), &original).unwrap();

        assert!(parse_csproj_manifest(root.path()).is_err());
        assert!(write_csproj_manifest(root.path(), &Manifest::new("app", Ecosystem::Lib)).is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join("App.csproj")).unwrap(),
            original,
            "conditional graph refusal must leave csproj unchanged"
        );
    }
}

#[tokio::test]
async fn dotnet_resolve_gate_refuses_ambiguous_root_projects_before_network() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("App.csproj"), "<Project/>").unwrap();
    std::fs::write(root.path().join("Tests.csproj"), "<Project/>").unwrap();
    let adapter =
        crate::adapter_for_language(LibLanguage::DotNet, root.path(), None, None).unwrap();

    let probe_error = adapter.probe_dependency_resolver().unwrap_err();
    assert!(
        probe_error
            .to_string()
            .contains("exactly one regular root-level .csproj")
    );

    let resolve_error = adapter
        .resolve(&mgc_types::Manifest::new(
            "ambiguous",
            mgc_types::Ecosystem::Lib,
        ))
        .await
        .unwrap_err();
    assert!(
        resolve_error
            .to_string()
            .contains("exactly one regular root-level .csproj")
    );
}

#[cfg(unix)]
#[test]
fn native_maven_writer_refuses_symlink_manifest_without_touching_target() {
    use crate::manifest::write_pom_manifest;
    use mgc_types::{Ecosystem, Manifest};
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("important.xml");
    std::fs::write(&target, "outside sentinel").unwrap();
    symlink(&target, project.path().join("pom.xml")).unwrap();

    let result = write_pom_manifest(project.path(), &Manifest::new("demo", Ecosystem::Lib));
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "outside sentinel");
    assert!(
        std::fs::symlink_metadata(project.path().join("pom.xml"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn dotnet_native_ownership_and_writer_ignore_symlink_csproj() {
    use crate::manifest::write_csproj_manifest;
    use mgc_types::{Ecosystem, Manifest};
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("important.csproj");
    std::fs::write(&target, "<Project>outside sentinel</Project>").unwrap();
    symlink(&target, project.path().join("Demo.csproj")).unwrap();

    assert_eq!(
        dependency_manifest_format(project.path(), LibLanguage::DotNet),
        None
    );
    let result = write_csproj_manifest(project.path(), &Manifest::new("demo", Ecosystem::Lib));
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "<Project>outside sentinel</Project>"
    );
}

#[cfg(unix)]
#[test]
fn maven_atomic_rewrite_preserves_existing_manifest_mode() {
    use crate::manifest::write_pom_manifest;
    use mgc_types::{Ecosystem, Manifest};
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().unwrap();
    let pom = project.path().join("pom.xml");
    std::fs::write(&pom, "<project/>").unwrap();
    std::fs::set_permissions(&pom, std::fs::Permissions::from_mode(0o600)).unwrap();
    write_pom_manifest(project.path(), &Manifest::new("demo", Ecosystem::Lib)).unwrap();
    assert_eq!(
        std::fs::metadata(pom).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn native_python_ownership_accepts_pep621_extras_as_inactive_metadata() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"demo\"\ndependencies = [\"requests>=2\"]\n",
    )
    .unwrap();
    assert!(supports_native_python_project(dir.path()));

    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname=\"demo\"\ndependencies=[\"requests>=2\"]\n[project.optional-dependencies]\ntest=[\"pytest>=8\"]\nmodel=[\"torch>=2\"]\n",
    )
    .unwrap();
    assert!(supports_native_python_project(dir.path()));
    let manifest = parse_pyproject_manifest(dir.path()).unwrap();
    assert!(manifest.find_dep("requests").is_some());
    assert!(manifest.find_dep("pytest").is_none());
    assert!(manifest.find_dep("torch").is_none());
    write_pyproject_manifest(dir.path(), &manifest).unwrap();
    let rewritten = std::fs::read_to_string(dir.path().join("pyproject.toml")).unwrap();
    assert!(
        rewritten.contains("pytest>=8"),
        "extras metadata was lost: {rewritten}"
    );
    assert!(
        rewritten.contains("torch>=2"),
        "extras metadata was lost: {rewritten}"
    );

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
        "[project]\nname=\"demo\"\ndynamic=[\"dependencies\"]\n",
        "[project]\nname=\"demo\"\n[tool.poetry.dependencies]\nrequests=\"^2\"\n",
        "[project]\nname=\"demo\"\n[tool.uv.sources]\nrequests={git=\"https://example.invalid/r.git\"}\n",
        "[project]\nname=\"demo\"\n[dependency-groups]\ntest=[\"pytest\"]\n",
    ] {
        std::fs::write(dir.path().join("pyproject.toml"), source).unwrap();
        assert!(!supports_native_python_project(dir.path()), "{source}");
    }
}

#[test]
fn native_python_lane_rejects_unmodeled_pep508_dependency_semantics() {
    let dir = tempfile::tempdir().unwrap();
    for dependency in [
        "requests[socks]>=2",
        "requests>=2;python_version<'3.12'",
        "demo @ https://example.invalid/demo.whl",
        "requests>=2,<3",
    ] {
        std::fs::write(
            dir.path().join("pyproject.toml"),
            format!("[project]\nname = \"demo\"\ndependencies = [{dependency:?}]\n"),
        )
        .unwrap();
        assert!(
            !supports_native_python_project(dir.path()),
            "unmodeled PEP 508 requirement must not enter native lane: {dependency}"
        );
        assert!(
            parse_pyproject_manifest(dir.path()).is_err(),
            "parser must refuse to flatten requirement: {dependency}"
        );
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

#[test]
fn pom_writer_preserves_non_runtime_dependencies_and_existing_metadata() {
    use crate::manifest::write_pom_manifest;
    use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pom.xml"),
        r#"<project>
  <dependencies>
    <dependency>
      <groupId>org.example</groupId><artifactId>test-only</artifactId><version>1.0.0</version>
      <scope>test</scope>
    </dependency>
    <dependency>
      <groupId>org.example</groupId><artifactId>runtime-lib</artifactId><version>1.0.0</version>
      <exclusions><exclusion><groupId>bad.example</groupId><artifactId>bad-child</artifactId></exclusion></exclusions>
    </dependency>
    <dependency>
      <groupId>org.example</groupId><artifactId>removed-runtime</artifactId><version>1.0.0</version>
    </dependency>
    <dependency>
      <groupId>org.example</groupId><artifactId>optional-lib</artifactId><version>1.0.0</version>
      <optional>true</optional>
    </dependency>
  </dependencies>
</project>
"#,
    )
    .unwrap();
    let mut manifest = Manifest::new("demo", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("org.example:runtime-lib").unwrap(),
            VersionRange::parse("2.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );

    write_pom_manifest(dir.path(), &manifest).unwrap();

    let body = std::fs::read_to_string(dir.path().join("pom.xml")).unwrap();
    assert!(
        body.contains("<scope>test</scope>"),
        "test dependency survives: {body}"
    );
    assert!(
        body.contains("<optional>true</optional>"),
        "optional dependency survives: {body}"
    );
    assert!(
        body.contains("<artifactId>bad-child</artifactId>"),
        "exclusion survives: {body}"
    );
    assert!(
        body.contains("<version>2.0.0</version>"),
        "owned pin updates: {body}"
    );
    assert!(
        !body.contains("removed-runtime"),
        "stale runtime pin removed: {body}"
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
