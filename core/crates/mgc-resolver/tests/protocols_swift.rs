//! SwiftPM registry engine tests — hermetic (mockito + local git fixture).
//! Test engine SwiftPM registry — hermetic (mockito + git fixture cục bộ).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{RegistryProtocol, SwiftRegistryProtocol};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping swift mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

/// Hand-assembled store-method zip (no zip-writer crate) — the archive
/// carries the package's Package.swift at the ROOT (SPM registry shape).
/// Zip method store tự ghép (không crate zip-writer) — archive mang
/// Package.swift của package ở GỐC (shape registry SPM).
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

const LIB_PACKAGE_SWIFT: &str = r#"// swift-tools-version:5.9
let package = Package(
    name: "Lib",
    dependencies: [
        .package(id: "other.lib", from: "2.0.0"),
        .package(url: "https://github.com/example/gitdep.git", exact: "1.0.0")
    ]
)"#;

#[tokio::test]
async fn swift_registry_roundtrip_resolve_verify_materialize() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let archive = build_archive(&[
        ("Package.swift", LIB_PACKAGE_SWIFT.as_bytes()),
        (
            "Sources/Lib/Lib.swift",
            b"export func hello() {}".as_slice(),
        ),
    ]);
    let sha = sha256_hex(&archive);

    server
        .mock("GET", "/scope/lib/scope.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.0.0","1.2.0","2.0.0"]}"#)
        .create_async()
        .await;
    // Highest matching wins (SwiftPM semantics): from:1.0.0 + 2.0.0 → 2.0.0.
    // (Cao nhất khớp thắng (ngữ nghĩa SwiftPM): from:1.0.0 + 2.0.0 → 2.0.0.)
    server
        .mock("GET", "/scope/lib/1.2.0.sha256")
        .with_status(200)
        .with_body(format!("{sha}\n"))
        .create_async()
        .await;
    let zip_mock = server
        .mock("GET", "/scope/lib/1.2.0.zip")
        .with_status(200)
        .with_body(archive.as_slice())
        .create_async()
        .await;

    let protocol = SwiftRegistryProtocol::with_registry(&base);
    let entry = protocol.resolve("scope/lib", "from:1.0.0").await.unwrap();
    // `from:1.0.0` is upToNextMajor (>=1.0.0, <2.0.0) → the highest
    // matching is 1.2.0, never 2.0.0.
    // (from:1.0.0 là upToNextMajor (>=1.0.0, <2.0.0) → cao nhất khớp là
    // 1.2.0, không bao giờ 2.0.0.)
    assert_eq!(entry.version, "1.2.0");
    assert_eq!(entry.sha256, sha);
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "swift-registry:scope"),
        "{:?}",
        entry.extra_markers
    );
    // Transitive deps from the archive's Package.swift.
    // (Dep bắc cầu từ Package.swift trong archive.)
    assert_eq!(
        entry.deps,
        vec![
            ("other/lib".to_string(), "from:2.0.0".to_string()),
            (
                "github.com/example/gitdep".to_string(),
                "exact:1.0.0".to_string()
            ),
        ]
    );

    // download() serves the resolve-cached bytes (single fetch).
    // (download() phục vụ byte đã cache lúc resolve (một lần tải).)
    let downloaded = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &downloaded).unwrap();
    zip_mock.assert_async().await;

    // Checksum mismatch fails closed.
    // (Checksum lệch fail-closed.)
    let mut tampered = entry.clone();
    tampered.sha256 = "0".repeat(64);
    let err = protocol.verify(&tampered, &downloaded).unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");

    // Checkouts materialization: {root}/checkouts/scope.lib-1.2.0/.
    // (Materialize checkouts: {root}/checkouts/scope.lib-1.2.0/.)
    let root = tempfile::tempdir().unwrap();
    let checkout = protocol
        .materialize(&entry, &downloaded, root.path())
        .unwrap();
    assert_eq!(
        checkout,
        root.path().join("checkouts").join("scope.lib-1.2.0")
    );
    assert!(checkout.join("Package.swift").is_file());
    assert!(checkout.join("Sources/Lib/Lib.swift").is_file());
}

#[tokio::test]
async fn swift_registry_missing_checksum_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    server
        .mock("GET", "/scope/lib/scope.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.0.0"]}"#)
        .create_async()
        .await;
    // No .sha256 endpoint (404) — a registry without checksums is never
    // trusted.
    // (Không endpoint .sha256 (404) — registry không checksum không bao giờ
    // được tin.)
    let protocol = SwiftRegistryProtocol::with_registry(&base);
    let err = protocol
        .resolve("scope/lib", "from:1.0.0")
        .await
        .unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

#[tokio::test]
async fn swift_registry_without_scope_fails_closed() {
    let protocol = SwiftRegistryProtocol::with_registry("http://127.0.0.1:9");
    let err = protocol.resolve("lib", "from:1.0.0").await.unwrap_err();
    assert!(
        err.to_string().contains("scope/name"),
        "guidance must name the scope requirement: {err}"
    );
}

/// Local bare-work git fixture: init → commit → tag 1.0.0. Returns
/// (worktree, commit sha).
/// Git fixture cục bộ: init → commit → tag 1.0.0. Trả về (worktree, sha
/// commit).
fn local_git_repo() -> Option<(tempfile::TempDir, String)> {
    let tmp = tempfile::tempdir().ok()?;
    // The repo lives in a fixed-name subdir — the checkout name derives
    // from the last path segment (GitDep-<ver>).
    // (Repo nằm trong subdir tên cố định — tên checkout suy từ segment
    // path cuối (GitDep-<ver>).)
    let repo_dir = tmp.path().join("GitDep");
    std::fs::create_dir_all(&repo_dir).ok()?;
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo_dir)
            .env("GIT_AUTHOR_NAME", "mgc-test")
            .env("GIT_AUTHOR_EMAIL", "mgc@test.local")
            .env("GIT_COMMITTER_NAME", "mgc-test")
            .env("GIT_COMMITTER_EMAIL", "mgc@test.local")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q", "-b", "main"]);
    std::fs::write(
        repo_dir.join("Package.swift"),
        r#"// swift-tools-version:5.9
let package = Package(
    name: "GitDep",
    dependencies: [
        .package(id: "transitive.lib", from: "3.0.0")
    ]
)"#,
    )
    .unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    run(&["tag", "1.0.0"]);
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo_dir)
        .output()
        .expect("rev-parse");
    assert!(sha.status.success());
    Some((tmp, String::from_utf8_lossy(&sha.stdout).trim().to_string()))
}

#[tokio::test]
async fn swift_git_dep_clone_provenance_and_materialize() {
    let Some((repo, sha)) = local_git_repo() else {
        eprintln!("warning: skipping git fixture test (tempdir unavailable)");
        return;
    };
    let name = format!("file://{}", repo.path().join("GitDep").display());
    let protocol = SwiftRegistryProtocol::with_registry("http://127.0.0.1:9");

    // Tag-range selection from the local repo.
    // (Chọn tag theo range từ repo cục bộ.)
    let entry = protocol.resolve(&name, "tag:1.0.0").await.unwrap();
    assert_eq!(entry.version, "1.0.0");
    let commit = entry
        .extra_markers
        .iter()
        .find_map(|m| m.strip_prefix("git-commit:"))
        .unwrap()
        .to_string();
    assert_eq!(commit, sha, "provenance SHA must be the tagged commit");
    assert_eq!(
        entry.deps,
        vec![("transitive/lib".to_string(), "from:3.0.0".to_string())]
    );

    // verify(): the downloaded "artifact" is the pinned SHA — a moved ref
    // cannot pass.
    // (verify(): "artifact" tải về là SHA đã ghim — ref bị dịch không qua
    // được.)
    let downloaded = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &downloaded).unwrap();
    let tampered = b"ffffffffffffffffffffffffffffffffffffffff".to_vec();
    let err = protocol.verify(&entry, &tampered).unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");

    // materialize_git: checkout at the pinned SHA — expected match passes,
    // a moved-tag expectation fails closed.
    // (materialize_git: checkout tại SHA đã ghim — khớp kỳ vọng thì pass,
    // kỳ vọng tag-dịch fail-closed.)
    let root = tempfile::tempdir().unwrap();
    let checkout = protocol
        .materialize_git(&entry, Some(&sha), root.path())
        .unwrap();
    assert_eq!(
        checkout,
        root.path().join("checkouts").join("GitDep-1.0.0")
    );
    assert!(checkout.join("Package.swift").is_file());

    let err = protocol
        .materialize_git(&entry, Some(&"f".repeat(40)), root.path())
        .unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}
