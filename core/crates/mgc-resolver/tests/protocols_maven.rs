//! Maven repository engine tests — hermetic via mockito (no real network).
//! Test engine repository Maven — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{MavenProtocol, RegistryProtocol};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping maven mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

const METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.example</groupId>
  <artifactId>core</artifactId>
  <versioning>
    <latest>1.4.0</latest>
    <release>1.4.0</release>
    <versions>
      <version>1.2.0</version>
      <version>1.2.3</version>
      <version>1.4.0</version>
      <version>2.0.0</version>
    </versions>
    <lastUpdated>20240102030405</lastUpdated>
  </versioning>
</metadata>"#;

const POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>core</artifactId>
  <version>1.2.3</version>
  <packaging>jar</packaging>
  <properties>
    <util.version>4.5.6</util.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.managed</groupId><artifactId>managed-dep</artifactId><version>9.9.9</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId><artifactId>util</artifactId><version>1.2.3</version>
    </dependency>
    <dependency>
      <groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><version>2.0.9</version><scope>runtime</scope>
    </dependency>
    <dependency>
      <groupId>junit</groupId><artifactId>junit</artifactId><version>4.13.2</version><scope>test</scope>
    </dependency>
    <dependency>
      <groupId>com.example</groupId><artifactId>optional-dep</artifactId><version>0.1.0</version><optional>true</optional>
    </dependency>
    <dependency>
      <groupId>com.example</groupId><artifactId>prop-dep</artifactId><version>${util.version}</version>
    </dependency>
  </dependencies>
</project>"#;

#[tokio::test]
async fn maven_resolves_graph_filters_scopes_and_materializes_m2_layout() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let jar_bytes = b"FAKE_JAR_BYTES_TWO_POINT_THREE";
    let jar_sha256 = sha256_hex(jar_bytes);

    server
        .mock("GET", "/com/example/core/maven-metadata.xml")
        .with_status(200)
        .with_body(METADATA)
        .create_async()
        .await;
    let pom_mock = server
        .mock("GET", "/com/example/core/1.2.3/core-1.2.3.pom")
        .with_status(200)
        .with_body(POM)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/core/1.2.3/core-1.2.3.jar.sha256")
        .with_status(200)
        .with_body(jar_sha256.as_str())
        .create_async()
        .await;
    let jar_mock = server
        .mock("GET", "/com/example/core/1.2.3/core-1.2.3.jar")
        .with_status(200)
        .with_body(jar_bytes.as_slice())
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    // Pinned exact version via the `[x]` interval form.
    // (Ghim version chính xác qua dạng khoảng `[x]`.)
    let entry = protocol
        .resolve("com.example:core", "[1.2.3]")
        .await
        .unwrap();
    assert_eq!(entry.version, "1.2.3");
    assert_eq!(
        entry.deps,
        vec![
            ("com.example:util".to_string(), "1.2.3".to_string()),
            ("org.slf4j:slf4j-api".to_string(), "2.0.9".to_string()),
            ("com.example:prop-dep".to_string(), "4.5.6".to_string()),
        ],
        "only compile/runtime deps enter the graph; ${{}} properties resolve from <properties>"
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "scope-skip:test:junit:junit"),
        "test scope must be recorded: {:?}",
        entry.extra_markers
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "optional:com.example:optional-dep"),
        "optional dep must be recorded: {:?}",
        entry.extra_markers
    );
    assert!(
        !entry
            .extra_markers
            .iter()
            .any(|m| m.starts_with("unresolved-version:")),
        "no silent version drops remain: {:?}",
        entry.extra_markers
    );
    assert!(
        !entry.deps.iter().any(|(n, _)| n.contains("managed-dep")),
        "dependencyManagement entries are never real deps"
    );
    assert_eq!(entry.sha256, jar_sha256);

    let jar = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &jar).unwrap();
    pom_mock.assert_async().await;
    jar_mock.assert_async().await;

    let pom = protocol
        .download_pom("com.example", "core", &entry.version)
        .await
        .unwrap();
    let m2 = tempfile::tempdir().unwrap();
    let jar_path = protocol.materialize(&entry, &jar, &pom, m2.path()).unwrap();
    assert_eq!(
        jar_path,
        m2.path()
            .join("repository/com/example/core/1.2.3/core-1.2.3.jar")
    );
    assert!(
        m2.path()
            .join("repository/com/example/core/1.2.3/core-1.2.3.pom")
            .is_file()
    );
    let written = std::fs::read_to_string(
        m2.path()
            .join("repository/com/example/core/1.2.3/core-1.2.3.jar.sha256"),
    )
    .unwrap();
    assert_eq!(written, jar_sha256);
}

#[tokio::test]
async fn maven_sha1_fallback_verifies_and_tampered_jar_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let jar_bytes = b"LEGACY_JAR_SHA1_ONLY";
    // Real sha1 of the jar bytes (computed through the same digest trait).
    let jar_sha1 = {
        use sha1::Digest;
        let mut hasher = sha1::Sha1::new();
        hasher.update(jar_bytes);
        hex::encode(hasher.finalize())
    };

    server
        .mock("GET", "/com/legacy/old/maven-metadata.xml")
        .with_status(200)
        .with_body(
            "<metadata><versioning><versions><version>1.0.0</version></versions></versioning></metadata>",
        )
        .create_async()
        .await;
    server
        .mock("GET", "/com/legacy/old/1.0.0/old-1.0.0.pom")
        .with_status(200)
        .with_body("<project><packaging>jar</packaging><dependencies/></project>")
        .create_async()
        .await;
    // No .sha256 published → sha1 fallback.
    // (Không công bố .sha256 → fallback sha1.)
    server
        .mock("GET", "/com/legacy/old/1.0.0/old-1.0.0.jar.sha256")
        .with_status(404)
        .create_async()
        .await;
    server
        .mock("GET", "/com/legacy/old/1.0.0/old-1.0.0.jar.sha1")
        .with_status(200)
        .with_body(jar_sha1.as_str())
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let entry = protocol.resolve("com.legacy:old", "[1.0.0]").await.unwrap();
    assert!(
        entry.sha256.is_empty(),
        "sha1-only entry keeps sha256 empty"
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == &format!("checksum-sha1:{jar_sha1}")),
        "sha1 must be recorded: {:?}",
        entry.extra_markers
    );
    protocol.verify(&entry, jar_bytes).unwrap();

    let err = protocol.verify(&entry, b"TAMPERED").unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

#[tokio::test]
async fn maven_bom_packaging_skips_transitive_graph() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let bom = r#"<project>
      <packaging>pom</packaging>
      <dependencies>
        <dependency><groupId>com.example</groupId><artifactId>managed</artifactId><version>1.0.0</version></dependency>
      </dependencies>
    </project>"#;
    server
        .mock("GET", "/com/bom/bom/maven-metadata.xml")
        .with_status(200)
        .with_body("<metadata><versioning><versions><version>3.0.0</version></versions></versioning></metadata>")
        .create_async()
        .await;
    server
        .mock("GET", "/com/bom/bom/3.0.0/bom-3.0.0.pom")
        .with_status(200)
        .with_body(bom)
        .create_async()
        .await;
    server
        .mock("GET", "/com/bom/bom/3.0.0/bom-3.0.0.jar.sha256")
        .with_status(200)
        .with_body("b".repeat(64))
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let entry = protocol.resolve("com.bom:bom", "[3.0.0]").await.unwrap();
    assert!(
        entry.deps.is_empty(),
        "BOM must not pull a transitive graph"
    );
    assert!(
        entry.extra_markers.iter().any(|m| m == "bom-packaging"),
        "{:?}",
        entry.extra_markers
    );
}

#[tokio::test]
async fn maven_no_checksum_anywhere_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    server
        .mock("GET", "/com/none/art/maven-metadata.xml")
        .with_status(200)
        .with_body("<metadata><versioning><versions><version>1.0.0</version></versions></versioning></metadata>")
        .create_async()
        .await;
    server
        .mock("GET", "/com/none/art/1.0.0/art-1.0.0.pom")
        .with_status(200)
        .with_body("<project><packaging>jar</packaging></project>")
        .create_async()
        .await;
    server
        .mock("GET", "/com/none/art/1.0.0/art-1.0.0.jar.sha256")
        .with_status(404)
        .create_async()
        .await;
    server
        .mock("GET", "/com/none/art/1.0.0/art-1.0.0.jar.sha1")
        .with_status(404)
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let err = protocol
        .resolve("com.none:art", "[1.0.0]")
        .await
        .unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

const UNRESOLVABLE_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>badprop</artifactId>
  <version>1.0.0</version>
  <packaging>jar</packaging>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId><artifactId>mystery</artifactId><version>${nope.prop}</version>
    </dependency>
  </dependencies>
</project>"#;

#[tokio::test]
async fn maven_unresolvable_property_fails_closed() {
    // V1.2 (D0): a ${} no properties/parent can satisfy is an error
    // naming the property — never a silent graph hole.
    let Some(mut server) = mock_server().await else {
        return;
    };
    server
        .mock("GET", "/com/example/badprop/maven-metadata.xml")
        .with_status(200)
        .with_body(METADATA_FOR_BADPROP)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/badprop/1.0.0/badprop-1.0.0.pom")
        .with_status(200)
        .with_body(UNRESOLVABLE_POM)
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let err = protocol
        .resolve("com.example:badprop", "[1.0.0]")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("nope.prop"),
        "the error must name the property: {err}"
    );
}

const METADATA_FOR_BADPROP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.example</groupId>
  <artifactId>badprop</artifactId>
  <versioning>
    <latest>1.0.0</latest>
    <release>1.0.0</release>
    <versions>
      <version>1.0.0</version>
    </versions>
  </versioning>
</metadata>"#;

const MANAGED_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>managed</artifactId>
  <version>2.0.0</version>
  <packaging>jar</packaging>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.example</groupId><artifactId>managed-dep</artifactId><version>9.9.9</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId><artifactId>managed-dep</artifactId>
    </dependency>
  </dependencies>
</project>"#;

#[tokio::test]
async fn maven_managed_version_fills_missing() {
    // A version-less dep with a dependencyManagement entry resolves to
    // the managed version (no fetch beyond the POM itself).
    let Some(mut server) = mock_server().await else {
        return;
    };
    server
        .mock("GET", "/com/example/managed/maven-metadata.xml")
        .with_status(200)
        .with_body(METADATA_FOR_MANAGED)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/managed/2.0.0/managed-2.0.0.pom")
        .with_status(200)
        .with_body(MANAGED_POM)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/managed/2.0.0/managed-2.0.0.jar.sha256")
        .with_status(200)
        .with_body(sha256_hex(b"MANAGED_JAR").as_str())
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/managed/2.0.0/managed-2.0.0.jar.sha1")
        .with_status(404)
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let entry = protocol
        .resolve("com.example:managed", "[2.0.0]")
        .await
        .unwrap();
    assert_eq!(
        entry.deps,
        vec![("com.example:managed-dep".to_string(), "9.9.9".to_string())]
    );
}

const METADATA_FOR_MANAGED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.example</groupId>
  <artifactId>managed</artifactId>
  <versioning>
    <latest>2.0.0</latest>
    <release>2.0.0</release>
    <versions>
      <version>2.0.0</version>
    </versions>
  </versioning>
</metadata>"#;

const CHILD_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId><artifactId>parent</artifactId><version>1.0.0</version>
  </parent>
  <groupId>com.example</groupId>
  <artifactId>child</artifactId>
  <version>3.0.0</version>
  <packaging>jar</packaging>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId><artifactId>inherited</artifactId><version>${parent.dep.version}</version>
    </dependency>
  </dependencies>
</project>"#;

const PARENT_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>parent</artifactId>
  <version>1.0.0</version>
  <packaging>pom</packaging>
  <properties>
    <parent.dep.version>7.7.7</parent.dep.version>
  </properties>
</project>"#;

#[tokio::test]
async fn maven_parent_chain_resolves_property() {
    // The child's ${} comes from the parent POM: one lazy fetch, child
    // keys win, then the chain ends.
    let Some(mut server) = mock_server().await else {
        return;
    };
    server
        .mock("GET", "/com/example/child/maven-metadata.xml")
        .with_status(200)
        .with_body(METADATA_FOR_CHILD)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/child/3.0.0/child-3.0.0.pom")
        .with_status(200)
        .with_body(CHILD_POM)
        .create_async()
        .await;
    let parent_mock = server
        .mock("GET", "/com/example/parent/1.0.0/parent-1.0.0.pom")
        .with_status(200)
        .with_body(PARENT_POM)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/child/3.0.0/child-3.0.0.jar.sha256")
        .with_status(200)
        .with_body(sha256_hex(b"CHILD_JAR").as_str())
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/child/3.0.0/child-3.0.0.jar.sha1")
        .with_status(404)
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let entry = protocol
        .resolve("com.example:child", "[3.0.0]")
        .await
        .unwrap();
    assert_eq!(
        entry.deps,
        vec![("com.example:inherited".to_string(), "7.7.7".to_string())]
    );
    parent_mock.assert_async().await;
}

const METADATA_FOR_CHILD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.example</groupId>
  <artifactId>child</artifactId>
  <versioning>
    <latest>3.0.0</latest>
    <release>3.0.0</release>
    <versions>
      <version>3.0.0</version>
    </versions>
  </versioning>
</metadata>"#;

const EMPTY_VERSION_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>emptyver</artifactId>
  <version>1.0.0</version>
  <packaging>jar</packaging>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId><artifactId>hollow</artifactId><version></version>
    </dependency>
  </dependencies>
</project>"#;

#[tokio::test]
async fn maven_empty_version_is_missing_not_empty_edge() {
    // REVIEW fix: `<version></version>` used to slip through as an
    // empty-string graph edge; it must take the missing-version path
    // (managed lookup, then fail-closed) instead.
    let Some(mut server) = mock_server().await else {
        return;
    };
    server
        .mock("GET", "/com/example/emptyver/maven-metadata.xml")
        .with_status(200)
        .with_body(METADATA_FOR_EMPTYVER)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/emptyver/1.0.0/emptyver-1.0.0.pom")
        .with_status(200)
        .with_body(EMPTY_VERSION_POM)
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let err = protocol
        .resolve("com.example:emptyver", "[1.0.0]")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("no version"),
        "empty version must fail closed as missing: {err}"
    );
}

const METADATA_FOR_EMPTYVER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.example</groupId>
  <artifactId>emptyver</artifactId>
  <versioning>
    <latest>1.0.0</latest>
    <release>1.0.0</release>
    <versions>
      <version>1.0.0</version>
    </versions>
  </versioning>
</metadata>"#;

#[tokio::test]
async fn maven_unreachable_parent_fails_closed() {
    // A needed-but-unfetchable parent POM is a network failure, never a
    // silent skip back to the old marker behavior.
    let Some(mut server) = mock_server().await else {
        return;
    };
    server
        .mock("GET", "/com/example/child/maven-metadata.xml")
        .with_status(200)
        .with_body(METADATA_FOR_CHILD)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/child/3.0.0/child-3.0.0.pom")
        .with_status(200)
        .with_body(CHILD_POM)
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/parent/1.0.0/parent-1.0.0.pom")
        .with_status(404)
        .create_async()
        .await;

    let protocol = MavenProtocol::new(&server.url());
    let err = protocol
        .resolve("com.example:child", "[3.0.0]")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("parent POM"),
        "unreachable parent must fail closed: {err}"
    );
}
