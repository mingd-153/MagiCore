#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-iot-adapter — sát với src/lib.rs
//! Kiểm thử: detect_framework (ESP32-Rust, PlatformIO, Zephyr), board mapping, PackageAdapter trait.

use mgc_iot_adapter::{
    IotFramework, KNOWN_BOARDS, adapter_for, board_target, board_target_for_framework,
    boards_for_framework, detect_framework, generate_sbom, known_boards,
};
use mgc_types::PackageName;
use mgc_types::adapter::{AddOptions, PackageAdapter};
use mgc_types::capabilities::{AuditProvider, CoreIdent, DependencyResolver, ProjectDetector};
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-iot-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[cfg(unix)]
#[test]
fn framework_detection_does_not_follow_external_mgc_config_symlink() {
    let project = tmp("external-mgc-config-link");
    let external = tmp("external-mgc-config-target");
    std::fs::write(
        external.join("mgc.toml"),
        "[iot]\nframework = 'platformio'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(external.join("mgc.toml"), project.join("mgc.toml")).unwrap();

    assert_eq!(detect_framework(&project), None);
    let _ = std::fs::remove_dir_all(project);
    let _ = std::fs::remove_dir_all(external);
}

#[cfg(unix)]
#[test]
fn board_selection_does_not_follow_external_mgc_config_symlink() {
    let project = tmp("external-board-config-link");
    let external = tmp("external-board-config-target");
    std::fs::write(
        project.join("platformio.ini"),
        "[env:local]\nplatform = espressif32\n",
    )
    .unwrap();
    std::fs::write(
        external.join("mgc.toml"),
        "[iot]\nboard = 'external-board'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(external.join("mgc.toml"), project.join("mgc.toml")).unwrap();

    let adapter = adapter_for(&project).expect("PlatformIO manifest identifies the adapter");
    assert_eq!(adapter.board(&project), None);
    let _ = std::fs::remove_dir_all(project);
    let _ = std::fs::remove_dir_all(external);
}

// ── detect_framework ───────────────────────────────────────────────────────

#[test]
fn detect_platformio_via_ini() {
    let dir = tmp("pio");
    std::fs::write(
        dir.join("platformio.ini"),
        "[env:esp32dev]\nplatform = espressif32\n",
    )
    .unwrap();
    assert_eq!(detect_framework(&dir), Some(IotFramework::Platformio));
}

#[test]
fn detect_zephyr_via_west_yml() {
    let dir = tmp("zephyr");
    std::fs::write(dir.join("west.yml"), "manifest:\n  projects: []\n").unwrap();
    assert_eq!(detect_framework(&dir), Some(IotFramework::Zephyr));
}

#[test]
fn detect_esp32_rust_via_mgc_toml() {
    let dir = tmp("esp32-rust");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"iot\"\n\n[iot]\nframework = \"esp32-rust\"\n",
    )
    .unwrap();
    assert_eq!(detect_framework(&dir), Some(IotFramework::Esp32Rust));
}

#[test]
fn detect_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(detect_framework(&dir).is_none());
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_for_platformio() {
    let dir = tmp("af-pio");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.framework(), "platformio");
}

#[test]
fn adapter_for_returns_none_for_plain_dir() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).is_none());
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let dir = tmp("name-eco");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.name(), "iot");
    assert_eq!(format!("{:?}", a.ecosystem()), "Iot");
}

#[test]
fn can_handle_returns_true_for_iot_project() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert!(a.can_handle(&dir));
}

#[tokio::test]
async fn platformio_add_fails_closed_outside_explicit_cli_compatibility() {
    let dir = tmp("add-pio");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("arduino-json").unwrap();
    let error = a
        .add(&dir, &name, None, AddOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            core: "iot",
            capability: "add",
            ..
        }
    ));
}

#[tokio::test]
async fn zephyr_add_fails_closed_without_native_dependency_support() {
    let dir = tmp("add-zephyr");
    std::fs::write(dir.join("west.yml"), "manifest:\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("lvgl").unwrap();
    let err = a
        .add(&dir, &name, None, AddOptions::default())
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("MagiCore-owned") || msg.contains("unsupported"),
        "error must explain that adapter-level add is not native: {msg}"
    );
}

#[tokio::test]
async fn zephyr_list_fails_closed_instead_of_returning_an_empty_native_list() {
    let dir = tmp("list-zephyr");
    std::fs::write(dir.join("west.yml"), "manifest:\n  projects: []\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let error = adapter.list(&dir).await.unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            core: "iot",
            capability: "list",
            ..
        }
    ));
    assert!(
        !adapter
            .capabilities()
            .contains(&mgc_types::capabilities::Capability::ContentStoreProvider)
    );
    assert!(mgc_types::capabilities::ContentStoreProvider::probe_content_store(&adapter).is_err());
}

#[tokio::test]
async fn platformio_list_does_not_report_declared_ranges_as_installed_versions() {
    let dir = tmp("list-pio");
    std::fs::write(
        dir.join("platformio.ini"),
        "[env:board]\nlib_deps = bblanchon/ArduinoJson@^6\n",
    )
    .unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let error = adapter.list(&dir).await.unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            capability: "list",
            ..
        }
    ));
}

#[tokio::test]
async fn platformio_adapter_install_fails_closed_without_spawning_pio() {
    let dir = tmp("adapter-no-delegate");
    std::fs::write(dir.join("platformio.ini"), "[env:board]\n").unwrap();
    let adapter = adapter_for(&dir).unwrap();
    let graph = mgc_types::ResolvedGraph::empty();
    let error = mgc_types::capabilities::ContentStoreProvider::install(
        &adapter,
        &graph,
        &dir,
        mgc_types::adapter::InstallOptions::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
}

#[tokio::test]
async fn audit_returns_clean_for_iot_project() {
    let dir = tmp("audit-iot");
    std::fs::write(dir.join("platformio.ini"), "[env:esp32]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}

#[test]
fn generate_sbom_uses_lockfile_v2_fixture() {
    let mut lockfile = mgc_lockfile::Lockfile::new();
    lockfile.add_package(mgc_lockfile::Package::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        "https://example.com/test.tgz".to_string(),
        "blake3:test123".to_string(),
    ));
    let json = generate_sbom(&lockfile, mgc_sbom::SbomOptions::default()).unwrap();
    assert!(json.contains("CycloneDX"));
    assert!(json.contains("test-pkg"));
    assert!(json.contains("1.0.0"));
}

#[test]
fn embedded_board_registry_compatibility_api_matches_snapshot() {
    let compatibility_snapshot = KNOWN_BOARDS
        .iter()
        .map(|(id, chip, target)| (id.to_string(), chip.to_string(), target.to_string()))
        .collect::<Vec<_>>();

    assert_eq!(known_boards(), compatibility_snapshot);
    for (id, _, target) in compatibility_snapshot {
        assert_eq!(board_target(&id).as_deref(), Some(target.as_str()));
    }
}

#[test]
fn board_registry_filters_choices_by_framework_and_rejects_cross_framework_targets() {
    let ids = |framework: &str| {
        boards_for_framework(framework)
            .into_iter()
            .map(|board| board.id)
            .collect::<Vec<_>>()
    };

    assert_eq!(ids("esp32-rust"), ["esp32c3", "esp32s3", "esp32"]);
    assert_eq!(ids("platformio"), ["esp32dev", "nodemcu-32s"]);
    assert_eq!(ids("zephyr"), ["nrf52dk_nrf52832", "stm32f4_disc"]);
    assert_eq!(ids("zephyr-arm"), ids("zephyr"));
    assert!(ids("unknown-framework").is_empty());

    assert_eq!(
        board_target_for_framework("esp32-rust", "esp32c3").as_deref(),
        Some("riscv32imac-unknown-none-elf")
    );
    assert_eq!(
        board_target_for_framework("esp32-rust", "nrf52dk_nrf52832"),
        None
    );
    assert_eq!(
        board_target_for_framework("unknown-framework", "esp32c3"),
        None
    );
}

#[test]
fn platformio_esp32dev_registry_maps_to_its_esp32_target() {
    let board = boards_for_framework("platformio")
        .into_iter()
        .find(|board| board.id == "esp32dev")
        .expect("esp32dev is a registered PlatformIO board");

    assert_eq!(board.chip, "esp32");
    assert_eq!(board.target, "xtensa-esp32-none-elf");
}
