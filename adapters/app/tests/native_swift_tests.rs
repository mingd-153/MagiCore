#![cfg(test)]
#![allow(clippy::unwrap_used)]
// Edition 2024 makes env::set_var/remove_var unsafe — the env overrides
// below are test-only and each block documents its restoration.
// (Edition 2024 biến env::set_var/remove_var thành unsafe — các env override
// dưới đây chỉ trong test và mỗi block ghi rõ việc phục hồi.)
#![allow(unsafe_code)]

//! Native Swift registry tests — manifest parsing is static and must not
//! spawn SwiftPM; registry traffic is mocked.
//! Test registry Swift native — parse manifest tĩnh, không spawn SwiftPM;
//! traffic registry được mock.

use mgc_types::adapter::PackageAdapter;
use mgc_types::capabilities::{ContentStoreProvider, DependencyResolver, LockfileProvider};
/// Process-global PATH is mutated by every test here — the lock serializes
/// the env-sensitive sections (parallel tests would otherwise clobber each
/// other's PATH and hit the real toolchain).
/// PATH toàn cục process bị mỗi test ở đây đổi — lock tuần tự hóa phần
/// nhạy env (test song song nếu không sẽ ghi đè PATH của nhau và đụng
/// toolchain thật).
static PATH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping swift app test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

/// Hand-assembled store-method zip (no zip-writer crate).
/// Zip method store tự ghép (không crate zip-writer).
fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut centrals: Vec<Vec<u8>> = Vec::new();
    for (name, data) in entries {
        let local_offset = out.len() as u32;
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
        cd.extend_from_slice(&local_offset.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        centrals.push(cd);
    }
    let cd_offset = out.len() as u32;
    for cd in &centrals {
        out.extend_from_slice(cd);
    }
    let cd_size = out.len() as u32 - cd_offset;
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(centrals.len() as u16).to_le_bytes());
    out.extend_from_slice(&(centrals.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[tokio::test]
async fn app_swift_project_resolves_and_installs_natively() {
    let _path_guard = PATH_LOCK.lock().await;
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let archive = build_archive(&[(
        "Package.swift",
        b"// swift-tools-version:5.9\nlet package = Package(name: \"Lib\")\n".as_slice(),
    )]);
    let sha = mgc_resolver::protocols::sha256_hex(&archive);

    server
        .mock("GET", "/scope/lib/scope.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.0.0"]}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/scope/lib/1.0.0.sha256")
        .with_status(200)
        .with_body(format!("{sha}\n"))
        .create_async()
        .await;
    server
        .mock("GET", "/scope/lib/1.0.0.zip")
        .with_status(200)
        .with_body(archive.as_slice())
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Package.swift"),
        "// swift-tools-version:5.9\nlet package = Package(name: \"App\", dependencies: [.package(id: \"scope.lib\", from: \"1.0.0\")])\n",
    )
    .unwrap();
    let store_root = tempfile::tempdir().unwrap();

    // SAFETY: test-only env overrides (process-local, restored below).
    // (SAFETY: env override chỉ trong test (cục bộ process, phục hồi bên
    // dưới).)
    unsafe {
        std::env::set_var("MGC_SWIFT_REGISTRY_URL", &base);
        std::env::set_var("MGC_SWIFT_STORE_ROOT", store_root.path());
    }
    let result = async {
        let adapter = mgc_app_adapter::adapter_for(tmp.path()).expect("swift project detected");
        let manifest = adapter.parse_manifest(tmp.path()).await?;
        assert_eq!(
            manifest.dependencies.len(),
            1,
            "{:?}",
            manifest.dependencies
        );
        assert_eq!(manifest.dependencies[0].name.as_str(), "scope/lib");
        let graph = adapter.resolve(&manifest).await?;
        assert_eq!(graph.packages.len(), 1);
        assert_eq!(graph.packages[0].id.name_str(), "scope/lib");
        assert_eq!(graph.packages[0].id.version().to_string(), "1.0.0");
        let summary = adapter
            .install(&graph, tmp.path(), Default::default())
            .await?;
        assert_eq!(summary.added.len(), 1);
        // write_manifest stays an honest no-op for Swift (Package.swift is
        // Swift source) — the file content must survive untouched.
        // (write_manifest vẫn no-op trung thực cho Swift (Package.swift là
        // mã nguồn Swift) — nội dung file phải nguyên vẹn.)
        adapter.write_manifest(tmp.path(), &manifest).await?;
        Ok::<_, mgc_types::MgError>(())
    }
    .await;
    unsafe {
        std::env::remove_var("MGC_SWIFT_REGISTRY_URL");
        std::env::remove_var("MGC_SWIFT_STORE_ROOT");
    }
    result.unwrap();

    // Checkout materialized under the mgc Swift root + Package.resolved
    // exported into the project.
    // (Checkout materialize dưới gốc Swift của mgc + Package.resolved xuất
    // vào project.)
    assert!(
        store_root
            .path()
            .join("checkouts")
            .join("scope.lib-1.0.0")
            .join("Package.swift")
            .is_file(),
        "registry checkout must exist"
    );
    let resolved_text = std::fs::read_to_string(tmp.path().join("Package.resolved")).unwrap();
    let resolved_doc: serde_json::Value = serde_json::from_str(&resolved_text).unwrap();
    assert_eq!(resolved_doc["version"], 2, "{resolved_text}");
    assert_eq!(
        resolved_doc["pins"][0]["identity"], "scope.lib",
        "{resolved_text}"
    );
    assert_eq!(
        resolved_doc["pins"][0]["state"]["version"], "1.0.0",
        "{resolved_text}"
    );
    // The project manifest is never rewritten (Package.swift is Swift
    // source — mgc only reads it).
    // (Manifest project không bao giờ bị viết lại (Package.swift là mã
    // nguồn Swift — mgc chỉ đọc).)
    let manifest_after = std::fs::read_to_string(tmp.path().join("Package.swift")).unwrap();
    assert!(manifest_after.contains("let package = Package(name: \"App\", dependencies:"));
}

#[tokio::test]
async fn app_swift_package_resolved_pins_win_over_declared_requirement() {
    let _path_guard = PATH_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Package.swift"),
        "// swift-tools-version:5.9\nlet package = Package(name: \"App\", dependencies: [.package(id: \"scope.lib\", from: \"1.0.0\")])\n",
    )
    .unwrap();
    // Declared `from:1.0.0` but the resolved file pins 1.4.2 — the pin is
    // the lock.
    // (Khai báo `from:1.0.0` nhưng file resolved ghim 1.4.2 — pin là lock.)
    std::fs::write(
        tmp.path().join("Package.resolved"),
        r#"{"version":2,"pins":[{"identity":"scope.lib","kind":"registry","location":"registry+https://reg","state":{"version":"1.4.2"}}]}"#,
    )
    .unwrap();
    let parsed = async {
        let adapter = mgc_app_adapter::adapter_for(tmp.path()).unwrap();
        adapter.parse_manifest(tmp.path()).await
    }
    .await;
    let manifest = parsed.unwrap();
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(
        manifest.dependencies[0].range.as_str(),
        "exact:1.4.2",
        "the Package.resolved pin must override the declared requirement"
    );
}

#[tokio::test]
async fn app_swift_manifest_parses_without_spawning_toolchain() {
    let _path_guard = PATH_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Package.swift"),
        "// swift-tools-version:5.9\nlet package = Package(name: \"App\", dependencies: [.package(id: \"scope.lib\", from: \"1.0.0\")])\n",
    )
    .unwrap();
    // An empty PATH proves manifest parsing is in-process, not SwiftPM.
    let empty_dir = tempfile::tempdir().unwrap();
    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", empty_dir.path().as_os_str());
    }
    let parsed = async {
        let adapter = mgc_app_adapter::adapter_for(tmp.path()).unwrap();
        adapter.parse_manifest(tmp.path()).await
    }
    .await;
    unsafe {
        std::env::set_var("PATH", original_path);
    }
    let manifest = parsed.unwrap();
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(manifest.dependencies[0].name.as_str(), "scope/lib");
}
