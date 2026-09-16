#![cfg(test)]
#![allow(clippy::unwrap_used)]
// Edition 2024 makes env::set_var/remove_var unsafe — the env overrides
// below are test-only and each block documents its restoration.
// (Edition 2024 biến env::set_var/remove_var thành unsafe — các env
// override dưới đây chỉ trong test và mỗi block ghi rõ việc phục hồi.)
#![allow(unsafe_code)]

//! Native React Native layered wiring tests — hermetic (mock Maven +
//! mock CocoaPods CDN; the JS tier carries no deps so the web pipeline
//! short-circuits without network).
//! Test wiring phân tầng React Native native — hermetic (mock Maven +
//! mock CDN CocoaPods; tier JS không có dep nên pipeline web
//! short-circuit không mạng).

use mgc_types::adapter::PackageAdapter;
use mgc_types::capabilities::{ContentStoreProvider, DependencyResolver};
use sha1::Digest as _;

/// Process-global env (maven URLs / store roots) is mutated here —
/// serialize the env-sensitive sections across parallel tests.
/// Env toàn cục process (URL/store root maven) bị đổi ở đây — tuần tự hóa
/// phần nhạy env giữa các test song song.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

#[tokio::test]
async fn app_rn_project_resolves_all_three_tiers_natively() {
    let _env_guard = ENV_LOCK.lock().await;
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
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
        r#"{"name":"rnapp","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("gradle.lockfile"), GRADLE_LOCKFILE).unwrap();
    std::fs::write(
        tmp.path().join("Podfile.lock"),
        podfile_lock(&alamofire_sha1, &fbcore_sha1),
    )
    .unwrap();

    let maven_store = tempfile::tempdir().unwrap();
    let original_repo = std::env::var("MGC_MAVEN_REPO_URL").ok();
    let original_store = std::env::var("MGC_MAVEN_STORE_ROOT").ok();
    // SAFETY: test-only env overrides (process-local, restored below).
    // (SAFETY: env override chỉ trong test (cục bộ process, phục hồi bên
    // dưới).)
    unsafe {
        std::env::set_var("MGC_MAVEN_REPO_URL", &base);
        std::env::set_var("MGC_MAVEN_STORE_ROOT", maven_store.path());
        std::env::set_var("MGC_COCOAPODS_CDN_URL", &base);
    }
    let result = async {
        let adapter = mgc_app_adapter::adapter_for(tmp.path()).expect("RN project detected");
        let manifest = adapter.parse_manifest(tmp.path()).await?;
        let graph = adapter.resolve(&manifest).await?;
        // JS tier contributes no packages today (the RN package.json dep
        // parse is still the Issue #13 stub — the web delegate receives an
        // empty manifest and short-circuits); 1 Maven pin + 2 pods.
        // (Tier JS đóng góp 0 package hiện tại (parse dep package.json của
        // RN vẫn là stub Issue #13 — web delegate nhận manifest rỗng và
        // short-circuit); 1 pin Maven + 2 pod.)
        let names: Vec<String> = graph
            .packages
            .iter()
            .map(|p| p.id.name_str().to_string())
            .collect();
        assert_eq!(graph.packages.len(), 3, "{names:?}");
        assert!(names.contains(&"com.example:lib".to_string()), "{names:?}");
        assert!(names.contains(&"Alamofire".to_string()), "{names:?}");
        assert!(names.contains(&"FBCore".to_string()), "{names:?}");
        let summary = adapter
            .install(&graph, tmp.path(), Default::default())
            .await?;
        assert_eq!(summary.added.len(), 3, "{:?}", summary.added);
        Ok::<_, mgc_types::MgError>(())
    }
    .await;
    unsafe {
        match original_repo {
            Some(v) => std::env::set_var("MGC_MAVEN_REPO_URL", v),
            None => std::env::remove_var("MGC_MAVEN_REPO_URL"),
        }
        match original_store {
            Some(v) => std::env::set_var("MGC_MAVEN_STORE_ROOT", v),
            None => std::env::remove_var("MGC_MAVEN_STORE_ROOT"),
        }
        std::env::remove_var("MGC_COCOAPODS_CDN_URL");
    }
    result.unwrap();

    // ── Per-tier entries in mgc.lock ──
    let lock_text = std::fs::read_to_string(tmp.path().join("mgc.lock")).unwrap();
    let lock = mgc_lockfile::parse_lockfile(&lock_text).unwrap();
    let maven: Vec<&mgc_lockfile::Package> = lock
        .packages
        .iter()
        .filter(|p| p.ecosystem == mgc_lockfile::EcosystemTag::Maven)
        .collect();
    let pods: Vec<&mgc_lockfile::Package> = lock
        .packages
        .iter()
        .filter(|p| p.ecosystem == mgc_lockfile::EcosystemTag::CocoaPods)
        .collect();
    assert_eq!(maven.len(), 1, "{lock_text}");
    assert_eq!(maven[0].name, "com.example:lib");
    assert_eq!(maven[0].version, "1.2.3");
    assert!(
        maven[0]
            .markers
            .as_ref()
            .is_some_and(|m| m.iter().any(|x| x == "rn-tier:android")),
        "{:?}",
        maven[0].markers
    );
    assert_eq!(pods.len(), 2, "{lock_text}");
    let alamofire = pods.iter().find(|p| p.name == "Alamofire").unwrap();
    assert_eq!(alamofire.version, "5.6.4");
    assert_eq!(alamofire.dependencies, vec!["FBCore".to_string()]);
    assert!(
        alamofire
            .markers
            .as_ref()
            .is_some_and(|m| m.contains(&format!("podspec-sha1:{alamofire_sha1}"))),
        "{:?}",
        alamofire.markers
    );
    assert!(
        alamofire
            .markers
            .as_ref()
            .is_some_and(|m| m.iter().any(|x| x == "rn-tier:ios")),
        "{:?}",
        alamofire.markers
    );
    assert!(
        alamofire
            .registry
            .as_deref()
            .is_some_and(|r| r.starts_with("cocoapods://")),
        "{:?}",
        alamofire.registry
    );

    // ── Android materialization: m2 layout ──
    assert!(
        maven_store
            .path()
            .join("repository/com/example/lib/1.2.3/lib-1.2.3.jar")
            .is_file(),
        "jar must be materialized into the m2 store"
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
        r#"{"name":"rnapp","dependencies":{"react-native":"0.72.0"}}"#,
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
async fn app_rn_podfile_without_lock_skips_ios_tier_with_guidance() {
    let _env_guard = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"rnapp","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("Podfile"), "platform :ios, '13.0'\n").unwrap();

    let adapter = mgc_app_adapter::adapter_for(tmp.path()).expect("RN project detected");
    let manifest = adapter.parse_manifest(tmp.path()).await.unwrap();
    // No Podfile.lock → the iOS tier is honestly skipped (no error, no
    // silent empty iOS graph claim).
    // (Không Podfile.lock → tier iOS bị bỏ trung thực (không lỗi, không
    // tuyên bố graph iOS rỗng âm thầm).)
    let graph = adapter.resolve(&manifest).await.unwrap();
    assert!(graph.packages.is_empty(), "{:?}", graph.packages);
}
