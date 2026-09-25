//! Native engine wiring tests for the lib adapter — hermetic via mockito.
//! Test đi dây engine native cho lib adapter — hermetic qua mockito.

#![allow(clippy::unwrap_used)]
#![allow(unsafe_code)]

use mgc_lib_adapter::native::engine::resolve_with_protocol;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::CratesProtocol;
use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{RegistryProtocol, ResolvedEntry};
use mgc_types::{
    DependencyResolver, DependencySpec, Ecosystem, Manifest, PackageAdapter, PackageName,
    VersionRange,
};

struct CrossRootConflictProtocol;

#[async_trait::async_trait]
impl RegistryProtocol for CrossRootConflictProtocol {
    async fn resolve(&self, name: &str, range: &str) -> mgc_types::MgResult<ResolvedEntry> {
        let (version, deps) = match name {
            "root-a" => ("1.0.0", vec![("shared".to_string(), "^1".to_string())]),
            "root-b" => ("1.0.0", vec![("shared".to_string(), "^2".to_string())]),
            "shared" if range == "^1" => ("1.5.0", vec![]),
            "shared" if range == "^2" => ("2.1.0", vec![]),
            _ => {
                return Err(mgc_types::MgError::Other(format!(
                    "unexpected {name}@{range}"
                )));
            }
        };
        Ok(ResolvedEntry {
            name: name.to_string(),
            version: version.to_string(),
            deps,
            artifact_url: format!("https://registry.invalid/{name}/{version}"),
            sha256: "00".repeat(32),
            extra_markers: vec![],
        })
    }

    async fn download(&self, _entry: &ResolvedEntry) -> mgc_types::MgResult<Vec<u8>> {
        Ok(vec![])
    }
}

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping adapter wiring mock test (localhost bind blocked)");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

fn crate_line(name: &str, vers: &str, cksum: &str, deps: &str) -> String {
    format!(
        r#"{{"name":"{name}","vers":"{vers}","deps":[{deps}],"cksum":"sha256:{cksum}","features":{{}},"yanked":false,"links":null}}"#
    )
}

fn rust_manifest() -> Manifest {
    let mut manifest = Manifest::new("demo-lib", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("serde").unwrap(),
            VersionRange::parse("^1.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    manifest
}

#[tokio::test]
async fn resolve_with_protocol_builds_graph_and_v3_lock_entries() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let serde_bytes = b"SERDE_CRATE";
    let core_bytes = b"SERDE_CORE";
    let serde_cksum = sha256_hex(serde_bytes);
    let core_cksum = sha256_hex(core_bytes);
    let base = server.url();

    server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(crate_line(
            "serde",
            "1.0.219",
            &serde_cksum,
            r#"{"name":"serde_core","req":"1.0","features":[],"optional":false,"default_features":true,"target":null,"kind":"normal","package":null}"#,
        ))
        .create_async()
        .await;
    server
        .mock("GET", "/se/rd/serde_core")
        .with_status(200)
        .with_body(crate_line("serde_core", "1.0.5", &core_cksum, ""))
        .create_async()
        .await;

    let protocol = CratesProtocol::with_download_base(&base, &base);
    let resolution = resolve_with_protocol(
        &protocol,
        EcosystemTag::Rust,
        "crates://sparse+https://index.crates.io",
        &rust_manifest(),
    )
    .await
    .unwrap();

    // ResolvedGraph: serde + serde_core, correct versions and edges.
    assert_eq!(resolution.graph.packages.len(), 2);
    let serde = resolution
        .graph
        .packages
        .iter()
        .find(|p| p.id.name_str() == "serde")
        .unwrap();
    assert_eq!(serde.id.version().to_string(), "1.0.219");
    assert_eq!(serde.deps.len(), 1);
    assert_eq!(serde.deps[0].name_str(), "serde_core");
    assert_eq!(serde.deps[0].version().to_string(), "1.0.5");
    assert_eq!(serde.integrity, format!("sha256-{serde_cksum}"));

    // v3 lock entries: ecosystem/provenance/registry/artifact recorded.
    assert_eq!(resolution.lock_packages.len(), 2);
    let lock_serde = resolution
        .lock_packages
        .iter()
        .find(|p| p.name == "serde")
        .unwrap();
    assert_eq!(lock_serde.ecosystem, EcosystemTag::Rust);
    assert_eq!(
        lock_serde.registry.as_deref(),
        Some("crates://sparse+https://index.crates.io")
    );
    let provenance = lock_serde.provenance.as_ref().unwrap();
    assert_eq!(provenance.source_kind, "native-resolve");
    let artifact = lock_serde.artifact.as_ref().unwrap();
    assert_eq!(artifact.url, format!("{base}/serde/1.0.219/download"));
    assert_eq!(
        artifact.downloaded_from,
        "crates://sparse+https://index.crates.io"
    );
    // blake3 content_hash is filled at install, not resolve — honest empty.
    assert!(artifact.content_hash.is_empty());
    assert!(lock_serde.store_ref.is_none());
}

#[tokio::test]
async fn resolve_with_protocol_rejects_cross_root_version_conflicts() {
    let mut manifest = Manifest::new("demo-lib", Ecosystem::Lib);
    for name in ["root-a", "root-b"] {
        manifest.add_dep(
            DependencySpec::new(PackageName::new(name).unwrap(), VersionRange::star()),
            false,
            false,
            false,
        );
    }

    let error = resolve_with_protocol(
        &CrossRootConflictProtocol,
        EcosystemTag::Rust,
        "test://registry",
        &manifest,
    )
    .await
    .unwrap_err();

    assert!(matches!(error, mgc_types::MgError::DependencyConflict(_)));
    assert!(error.to_string().contains("ambiguous lock graph"));
}

#[tokio::test]
async fn lib_adapter_resolve_uses_native_engine_via_env() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let cksum = sha256_hex(b"SERDE_CRATE");
    server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(crate_line("serde", "1.0.219", &cksum, ""))
        .create_async()
        .await;

    // SAFETY: test-only env override to point the native engine at mockito;
    // the values are process-local and restored immediately after the call.
    unsafe {
        std::env::set_var("MGC_CRATES_INDEX_URL", server.url());
        std::env::set_var("MGC_CRATES_DOWNLOAD_URL", server.url());
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let adapter = mgc_lib_adapter::adapter_for(tmp.path(), None, None)
        .unwrap()
        .unwrap();
    let graph = adapter.resolve(&rust_manifest()).await.unwrap();

    assert_eq!(graph.packages.len(), 1);
    assert_eq!(graph.packages[0].id.name_str(), "serde");
    assert_eq!(graph.packages[0].id.version().to_string(), "1.0.219");

    unsafe {
        std::env::remove_var("MGC_CRATES_INDEX_URL");
        std::env::remove_var("MGC_CRATES_DOWNLOAD_URL");
    }
}

#[tokio::test]
async fn resolve_with_protocol_builds_go_module_graph_and_v3_lock_entries() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();

    // Zip for the module + its proxy artifacts.
    // (Zip cho module + các artifact proxy.)
    let zip_bytes = {
        // Minimal single-entry store zip, hand-assembled (no zip crate).
        // (Zip store một entry tối giản, ghép thủ công — không crate zip.)
        let name = "example.com/lib@v1.4.0/go.mod";
        let data = b"module example.com/lib\n";
        let mut out: Vec<u8> = Vec::new();
        let crc = mgc_resolver::protocols::zip_reader::crc32(data);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        let cd_offset = out.len() as u32;
        let mut cd = Vec::new();
        cd.extend_from_slice(b"PK\x01\x02");
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&crc.to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&cd);
        let cd_size = out.len() as u32 - cd_offset;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    };
    let zip_hash = sha256_hex(&zip_bytes);

    server
        .mock("GET", "/example.com/lib/@v/list")
        .with_status(200)
        .with_body("v1.4.0\n")
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/lib/@v/v1.4.0.info")
        .with_status(200)
        .with_body(r#"{"Version":"v1.4.0","Time":"2024-03-04T05:06:07Z"}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/lib/@v/v1.4.0.mod")
        .with_status(200)
        .with_body("module example.com/lib\n")
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/lib/@v/v1.4.0.ziphash")
        .with_status(200)
        .with_body(zip_hash.as_str())
        .create_async()
        .await;

    let protocol = mgc_resolver::protocols::GoModProtocol::with_sum_base(&base, &base);
    let mut manifest = Manifest::new("example.com/app", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("example.com/lib").unwrap(),
            VersionRange::parse("1.4.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let resolution = resolve_with_protocol(
        &protocol,
        EcosystemTag::Go,
        "go://proxy.golang.org",
        &manifest,
    )
    .await
    .unwrap();

    assert_eq!(resolution.graph.packages.len(), 1);
    let pkg = &resolution.graph.packages[0];
    assert_eq!(pkg.id.name_str(), "example.com/lib");
    assert_eq!(pkg.id.version().to_string(), "1.4.0");

    assert_eq!(resolution.lock_packages.len(), 1);
    let lock = &resolution.lock_packages[0];
    assert_eq!(lock.ecosystem, EcosystemTag::Go);
    assert_eq!(lock.registry.as_deref(), Some("go://proxy.golang.org"));
    assert_eq!(lock.integrity, format!("sha256-{zip_hash}"));
    assert_eq!(
        lock.provenance.as_ref().unwrap().source_kind,
        "native-resolve"
    );
    assert_eq!(
        lock.artifact.as_ref().unwrap().url,
        format!("{base}/example.com/lib/@v/v1.4.0.zip")
    );
}

#[tokio::test]
async fn lib_adapter_maven_pom_project_resolves_natively_via_env() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let jar_bytes = b"MOCKITO_JAR_BYTES";
    let jar_sha256 = sha256_hex(jar_bytes);
    let base = server.url();

    server
        .mock("GET", "/com/demo/core/maven-metadata.xml")
        .with_status(200)
        .with_body(
            "<metadata><versioning><versions><version>1.0.0</version></versions></versioning></metadata>",
        )
        .create_async()
        .await;
    server
        .mock("GET", "/com/demo/core/1.0.0/core-1.0.0.pom")
        .with_status(200)
        .with_body(
            "<project><packaging>jar</packaging><dependencies><dependency><groupId>com.demo</groupId><artifactId>util</artifactId><version>1.0.0</version></dependency></dependencies></project>",
        )
        .create_async()
        .await;
    server
        .mock("GET", "/com/demo/core/1.0.0/core-1.0.0.jar.sha256")
        .with_status(200)
        .with_body(jar_sha256.as_str())
        .create_async()
        .await;
    // Transitive dep com.demo:util — full mock set.
    // (Dep bắc cầu com.demo:util — bộ mock đầy đủ.)
    let util_bytes = b"MOCKITO_UTIL_JAR";
    let util_sha256 = sha256_hex(util_bytes);
    server
        .mock("GET", "/com/demo/util/maven-metadata.xml")
        .with_status(200)
        .with_body(
            "<metadata><versioning><versions><version>1.0.0</version></versions></versioning></metadata>",
        )
        .create_async()
        .await;
    server
        .mock("GET", "/com/demo/util/1.0.0/util-1.0.0.pom")
        .with_status(200)
        .with_body("<project><packaging>jar</packaging><dependencies/></project>")
        .create_async()
        .await;
    server
        .mock("GET", "/com/demo/util/1.0.0/util-1.0.0.jar.sha256")
        .with_status(200)
        .with_body(util_sha256.as_str())
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pom.xml"),
        "<project><groupId>com.demo</groupId><artifactId>app</artifactId><version>0.1.0</version><dependencies><dependency><groupId>com.demo</groupId><artifactId>core</artifactId><version>1.0.0</version></dependency></dependencies></project>",
    )
    .unwrap();

    // SAFETY: test-only env override to point the native engine at mockito;
    // process-local and restored immediately after the call.
    unsafe {
        std::env::set_var("MGC_MAVEN_REPO_URL", &base);
    }
    let adapter = mgc_lib_adapter::adapter_for(tmp.path(), None, None)
        .unwrap()
        .unwrap();
    let manifest = adapter.parse_manifest(tmp.path()).await.unwrap();
    assert_eq!(manifest.name, "com.demo:app");
    let graph = adapter.resolve(&manifest).await.unwrap();
    unsafe {
        std::env::remove_var("MGC_MAVEN_REPO_URL");
    }

    assert_eq!(graph.packages.len(), 2, "core + transitive util");
    assert_eq!(graph.packages[0].id.name_str(), "com.demo:core");
    assert_eq!(graph.packages[0].id.version().to_string(), "1.0.0");
    assert_eq!(graph.packages[1].id.name_str(), "com.demo:util");

    // Gradle projects fail closed with honest guidance.
    // (Project gradle fail-closed kèm hướng dẫn trung thực.)
    let gradle = tempfile::tempdir().unwrap();
    std::fs::write(gradle.path().join("build.gradle"), "plugins {}\n").unwrap();
    let gradle_adapter = mgc_lib_adapter::adapter_for(gradle.path(), None, None)
        .unwrap()
        .unwrap();
    let err = gradle_adapter
        .resolve(&mgc_types::Manifest::new("g", Ecosystem::Lib))
        .await
        .unwrap_err();
    assert!(
        matches!(err, mgc_types::MgError::Unsupported { .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn lib_adapter_csproj_project_resolves_natively_via_env() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    // Hand-assembled store-method nupkg (no zip crate).
    // (Nupkg method store tự ghép — không crate zip.)
    let nupkg = {
        let name = "Demo.Lib.nuspec";
        let data = "<package><metadata><id>Demo.Lib</id><dependencies /></metadata></package>"
            .to_string()
            .into_bytes();
        let mut out: Vec<u8> = Vec::new();
        let crc = mgc_resolver::protocols::zip_reader::crc32(&data);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&data);
        let cd_offset = out.len() as u32;
        let mut cd = Vec::new();
        cd.extend_from_slice(b"PK\x01\x02");
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&crc.to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&cd);
        let cd_size = out.len() as u32 - cd_offset;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    };
    let sha512_b64 = {
        use base64::Engine as _;
        use sha2::Digest as _;
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(&nupkg))
    };

    server
        .mock("GET", "/v3/index.json")
        .with_status(200)
        .with_body(format!(
            r#"{{"resources":[{{"@id":"{base}/reg/","@type":"RegistrationsBaseUrl/3.6.0"}},{{"@id":"{base}/flat/","@type":"PackageBaseAddress/3.0.0"}}]}}"#
        ))
        .create_async()
        .await;
    server
        .mock("GET", "/flat/demo.lib/index.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.2.3"]}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/reg/demo.lib/index.json")
        .with_status(200)
        // Live nuget.org registration leaves OMIT packageHash — the
        // resolver must follow catalogEntry @id to the catalog document.
        // (Registration thật không có packageHash — resolver phải theo @id.)
        .with_body(format!(
            r#"{{"items":[{{"items":[{{"catalogEntry":{{"@id":"{base}/catalog/demo.lib.1.2.3.json","id":"Demo.Lib","version":"1.2.3","listed":true}}}}]}}]}}"#
        ))
        .create_async()
        .await;
    server
        .mock("GET", "/catalog/demo.lib.1.2.3.json")
        .with_status(200)
        .with_body(format!(
            "{{\"packageHash\":\"{sha512_b64}\",\"packageHashAlgorithm\":\"SHA512\"}}"
        ))
        .create_async()
        .await;
    server
        .mock("GET", "/flat/demo.lib/1.2.3/demo.lib.nuspec")
        .with_status(200)
        .with_body(r#"<package><metadata><dependencies /></metadata></package>"#)
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Demo.App.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk"><ItemGroup><PackageReference Include="Demo.Lib" Version="1.2.3" /></ItemGroup></Project>"#,
    )
    .unwrap();

    // SAFETY: test-only env override to point the native engine at mockito;
    // process-local and restored immediately after the call.
    unsafe {
        std::env::set_var("MGC_NUGET_INDEX_URL", format!("{base}/v3/index.json"));
    }
    let adapter = mgc_lib_adapter::adapter_for(tmp.path(), None, None)
        .unwrap()
        .unwrap();
    let manifest = adapter.parse_manifest(tmp.path()).await.unwrap();
    assert_eq!(manifest.name, "Demo.App");
    let graph = adapter.resolve(&manifest).await.unwrap();
    unsafe {
        std::env::remove_var("MGC_NUGET_INDEX_URL");
    }
    assert_eq!(graph.packages.len(), 1);
    assert_eq!(graph.packages[0].id.name_str(), "Demo.Lib");
    assert_eq!(graph.packages[0].id.version().to_string(), "1.2.3");
}
