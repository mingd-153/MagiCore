//! Tests cho game/install/unity — tách khỏi src theo RULE §5.
// (Tests for game/install/unity — split per RULE §5.)

use super::*;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

fn write_project(root: &std::path::Path, lock_json: Option<&str>) {
    let packages_dir = root.join("Packages");
    std::fs::create_dir_all(&packages_dir).unwrap();
    std::fs::write(packages_dir.join("manifest.json"), "{}").unwrap();
    if let Some(json) = lock_json {
        std::fs::write(packages_dir.join("packages-lock.json"), json).unwrap();
    }
}

#[tokio::test]
async fn test_install_unity_no_manifest_fails() {
    let tmp = tmp();
    assert!(install_dependencies(tmp.path()).await.is_err());
}

#[tokio::test]
async fn test_install_unity_no_lock_first_install() {
    let tmp = tmp();
    write_project(tmp.path(), None);

    let (packages, bytes, lock_present) = install_dependencies(tmp.path()).await.unwrap();
    assert_eq!(packages.len(), 0);
    assert_eq!(bytes, 0);
    assert!(!lock_present, "chưa có lockfile → cờ phải false");
}

#[tokio::test]
async fn test_install_unity_with_lock_lists_packages() {
    let tmp = tmp();
    write_project(
        tmp.path(),
        Some(
            r#"{"dependencies":{"com.unity.textmeshpro":{"version":"3.0.6","depth":0,"source":"registry","hash":"abc123"}}}"#,
        ),
    );

    // Lưu ý ngữ nghĩa: bool thứ 3 = "lockfile tồn tại + parse OK",
    // KHÔNG phải "hash đã đối chiếu với cache" (cảnh báo P2 được phát lúc runtime).
    // (Semantics: 3rd bool = lockfile present + parseable, NOT hashes checked.)
    let (packages, _, lock_present) = install_dependencies(tmp.path()).await.unwrap();
    assert_eq!(packages.len(), 1);
    assert!(lock_present);
    assert!(packages[0].contains("textmeshpro"));
}

#[test]
fn test_parse_packages_lock() {
    let tmp = tmp();
    let lock_path = tmp.path().join("packages-lock.json");

    let lock_json = r#"{
        "dependencies": {
            "com.unity.test": {
                "version": "1.0.0",
                "depth": 0,
                "source": "registry",
                "hash": "sha256-abc123"
            }
        }
    }"#;

    std::fs::write(&lock_path, lock_json).unwrap();

    let lock = parse_packages_lock(&lock_path).unwrap();
    assert_eq!(lock.dependencies.len(), 1);
    assert_eq!(lock.dependencies["com.unity.test"].version, "1.0.0");
    assert_eq!(
        lock.dependencies["com.unity.test"].hash.as_deref(),
        Some("sha256-abc123")
    );
}
