#![cfg(test)]
#![allow(clippy::unwrap_used)]
// Edition 2024 makes env::set_var/remove_var unsafe — the env overrides
// below are test-only and each block documents its restoration.
// (Edition 2024 biến env::set_var/remove_var thành unsafe — các env
// override dưới đây chỉ trong test và mỗi block ghi rõ việc phục hồi.)
#![allow(unsafe_code)]

//! Native React Native layered wiring tests — all three registries are mocked.
//! Test wiring React Native — cả ba registry đều được mock.

use mgc_types::adapter::PackageAdapter;
use mgc_types::capabilities::{ContentStoreProvider, DependencyResolver};
use sha1::Digest as _;
use tar::{Builder, Header};

/// Process-global env (registry URLs / store roots) is mutated here —
/// serialize the env-sensitive sections across parallel tests.
/// Env toàn cục process (URL registry/store root) bị đổi ở đây — tuần tự hóa
/// phần nhạy env giữa các test song song.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct EnvRestoreGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl EnvRestoreGuard {
    fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }
}

impl Drop for EnvRestoreGuard {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            // SAFETY: the test holds ENV_LOCK for the complete scope, so no
            // other test in this process can concurrently inspect/mutate
            // these process-global environment variables.
            // (AN TOÀN: test giữ ENV_LOCK suốt scope.)
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping RN mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

const GRADLE_LOCKFILE: &str = "# This is a Gradle generated file for dependency locking.\n# Manual edits can break the build.\ncom.example:lib:1.2.3:releaseCompileClasspath\n";

fn podfile_lock(alamofire_sha1: &str, fbcore_sha1: &str) -> String {
    format!(
        "PODS:\n  - Alamofire (5.6.4):\n    - FBCore (= 1.0.0)\n  - FBCore (1.0.0)\n\nDEPENDENCIES:\n  - Alamofire (~> 5.6)\n\nSPEC CHECKSUMS:\n  Alamofire: {alamofire_sha1}\n  FBCore: {fbcore_sha1}\n\nPODFILE CHECKSUM: dddddddddddddddddddddddddddddddddddddddd\n"
    )
}

const ALAMOFIRE_SPEC: &str =
    r#"{"name":"Alamofire","version":"5.6.4","dependencies":{"FBCore":["= 1.0.0"]}}"#;
const FBCORE_SPEC: &str = r#"{"name":"FBCore","version":"1.0.0","dependencies":{}}"#;

fn npm_tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let encoder =
        flate2::GzBuilder::new().write(temp.reopen().unwrap(), flate2::Compression::default());
    let mut builder = Builder::new(encoder);
    for (path, bytes) in files {
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("package/{path}"), *bytes)
            .unwrap();
    }
    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
    std::fs::read(temp.path()).unwrap()
}

fn npm_integrity(bytes: &[u8]) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(bytes))
    )
}

#[tokio::test]
async fn app_rn_multi_tier_install_fails_closed_without_atomic_commit() {
    let _env_guard = ENV_LOCK.lock().await;
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let rn_tarball = npm_tarball(&[(
        "package.json",
        br#"{"name":"react-native","version":"0.72.0"}"#,
    )]);
    let rn_integrity = npm_integrity(&rn_tarball);
    let rn_packument = serde_json::json!({
        "name": "react-native",
        "versions": {
            "0.72.0": {
                "name": "react-native",
                "version": "0.72.0",
                "dependencies": {},
                "dist": {
                    "tarball": format!("{base}/react-native/-/react-native-0.72.0.tgz"),
                    "integrity": rn_integrity
                }
            }
        },
        "dist-tags": {"latest": "0.72.0"},
        "time": {"0.72.0": "2024-01-01T00:00:00.000Z"}
    });
    server
        .mock("GET", "/react-native")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(rn_packument.to_string())
        .expect(1)
        .create_async()
        .await;
    server
        .mock("GET", "/react-native/-/react-native-0.72.0.tgz")
        .with_status(200)
        .with_body(rn_tarball)
        .expect(1)
        .create_async()
        .await;
    let jar = b"JAR-BYTES-1.2.3".to_vec();
    let jar_sha256 = mgc_resolver::protocols::sha256_hex(&jar);

    // ── Maven mock (Android tier) ──
    server
        .mock("GET", "/com/example/lib/maven-metadata.xml")
        .with_status(200)
        .with_body(
            "<metadata><versioning><versions><version>1.2.3</version></versions></versioning></metadata>",
        )
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/lib/1.2.3/lib-1.2.3.pom")
        .with_status(200)
        .with_body("<project><groupId>com.example</groupId><artifactId>lib</artifactId><version>1.2.3</version></project>")
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/lib/1.2.3/lib-1.2.3.jar.sha256")
        .with_status(200)
        .with_body(format!("{jar_sha256}\n"))
        .create_async()
        .await;
    server
        .mock("GET", "/com/example/lib/1.2.3/lib-1.2.3.jar")
        .with_status(200)
        .with_body(jar.as_slice())
        .create_async()
        .await;

    // ── CocoaPods CDN mock (iOS tier) — sha1 of each spec body ──
    let alamofire_sha1 = hex::encode(sha1::Sha1::digest(ALAMOFIRE_SPEC.as_bytes()));
    let fbcore_sha1 = hex::encode(sha1::Sha1::digest(FBCORE_SPEC.as_bytes()));
    server
        .mock("GET", "/a/Alamofire/5.6.4/Alamofire.podspec.json")
        .with_status(200)
        .with_body(ALAMOFIRE_SPEC)
        .create_async()
        .await;
    server
        .mock("GET", "/f/FBCore/1.0.0/FBCore.podspec.json")
        .with_status(200)
        .with_body(FBCORE_SPEC)
        .create_async()
        .await;

    // ── Project fixture ──
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"rnapp","version":"1.0.0","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("gradle.lockfile"), GRADLE_LOCKFILE).unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        podfile_lock(&alamofire_sha1, &fbcore_sha1),
    )
    .unwrap();

    let maven_store = tempfile::tempdir().unwrap();
    let _restore_env = EnvRestoreGuard::capture(&[
        "MGC_MAVEN_REPO_URL",
        "MGC_MAVEN_STORE_ROOT",
        "MGC_COCOAPODS_CDN_URL",
        "MAGICORE_WEB_REGISTRY_URL",
        "MAGICORE_WEB_ALLOWED_REGISTRIES",
    ]);
    // SAFETY: test-only env overrides (process-local, restored below).
    // (SAFETY: env override chỉ trong test (cục bộ process, phục hồi bên
    // dưới).)
    unsafe {
        std::env::set_var("MGC_MAVEN_REPO_URL", &base);
        std::env::set_var("MGC_MAVEN_STORE_ROOT", maven_store.path());
        std::env::set_var("MGC_COCOAPODS_CDN_URL", &base);
        std::env::set_var("MAGICORE_WEB_REGISTRY_URL", &base);
        std::env::set_var("MAGICORE_WEB_ALLOWED_REGISTRIES", &base);
    }
    let result = async {
        let adapter = mgc_app_adapter::adapter_for(tmp.path()).expect("RN project detected");
        let manifest = adapter.parse_manifest(tmp.path()).await?;
        let graph = adapter.resolve(&manifest).await?;
        // The JS tier resolves the declared React Native dependency through
        // the mocked npm registry; Android and iOS add one Maven pin and two
        // CocoaPods entries.
        // (Tier JS resolve dependency React Native qua npm mock; Android và
        // iOS thêm một Maven pin cùng hai entry CocoaPods.)
        let names: Vec<String> = graph
            .packages
            .iter()
            .map(|p| p.id.name_str().to_string())
            .collect();
        assert_eq!(graph.packages.len(), 4, "{names:?}");
        assert!(names.contains(&"react-native".to_string()), "{names:?}");
        assert!(names.contains(&"com.example:lib".to_string()), "{names:?}");
        assert!(names.contains(&"Alamofire".to_string()), "{names:?}");
        assert!(names.contains(&"FBCore".to_string()), "{names:?}");
        let error = adapter
            .install(&graph, tmp.path(), Default::default())
            .await
            .unwrap_err();
        assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
        Ok::<_, mgc_types::MgError>(())
    }
    .await;
    result.unwrap();

    assert!(
        !tmp.path().join("mgc.lock").exists(),
        "unsupported multi-tier install must not publish a partial lock"
    );
    assert!(
        !tmp.path().join("Package.resolved").exists(),
        "unsupported multi-tier install must not publish a partial Swift lock"
    );
}

#[tokio::test]
async fn app_rn_podspec_checksum_mismatch_fails_closed() {
    let _env_guard = ENV_LOCK.lock().await;
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    // The lock records a checksum that does NOT match the served spec.
    // (Lock ghi checksum KHÔNG khớp spec được phục vụ.)
    server
        .mock("GET", "/a/Alamofire/5.6.4/Alamofire.podspec.json")
        .with_status(200)
        .with_body(ALAMOFIRE_SPEC)
        .create_async()
        .await;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"rnapp","version":"1.0.0","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        podfile_lock(
            "0000000000000000000000000000000000000000",
            "1111111111111111111111111111111111111111",
        ),
    )
    .unwrap();

    let original_cdn = std::env::var("MGC_COCOAPODS_CDN_URL").ok();
    // SAFETY: test-only env override (process-local, restored below).
    // (SAFETY: env override chỉ trong test (cục bộ process, phục hồi bên
    // dưới).)
    unsafe {
        std::env::set_var("MGC_COCOAPODS_CDN_URL", &base);
    }
    let result = async {
        let adapter = mgc_app_adapter::adapter_for(tmp.path()).expect("RN project detected");
        let manifest = adapter.parse_manifest(tmp.path()).await?;
        adapter.resolve(&manifest).await
    }
    .await;
    unsafe {
        match original_cdn {
            Some(v) => std::env::set_var("MGC_COCOAPODS_CDN_URL", v),
            None => std::env::remove_var("MGC_COCOAPODS_CDN_URL"),
        }
    }
    let err = result.unwrap_err();
    assert!(
        matches!(err, mgc_types::MgError::Integrity(_)),
        "a podspec sha1 mismatch must fail closed: {err:?}"
    );
}

#[tokio::test]
async fn app_rn_podfile_without_lock_fails_closed() {
    let _env_guard = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"rnapp","version":"1.0.0","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("Podfile"), "platform :ios, '13.0'\n").unwrap();

    let adapter = mgc_app_adapter::adapter_for(tmp.path()).expect("RN project detected");
    let manifest = adapter.parse_manifest(tmp.path()).await.unwrap();
    // No Podfile.lock → the iOS tier has no verifiable closure, so resolve
    // MUST fail closed (V1.2: skip paths must never read as pass).
    // (Không Podfile.lock → tier iOS không có bao đóng kiểm chứng được,
    // resolve PHẢI fail-closed.)
    let err = adapter.resolve(&manifest).await.unwrap_err();
    assert!(
        matches!(err, mgc_types::MgError::Unsupported { .. }),
        "a Podfile without Podfile.lock must fail closed as Unsupported: {err:?}"
    );
    assert!(
        err.to_string().contains("Podfile.lock"),
        "the error must tell the user how to recover: {err}"
    );
}
