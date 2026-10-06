use super::*;

#[tokio::test]
async fn install_at_with_no_packages_succeeds_without_adapter() {
    // Regression (T0.3-hardware): the old trailing `install_with_adapter`
    // call failed with Unsupported AFTER doing the work (side effects +
    // error). Template-only install completes with Ok and no adapter.
    // (Hồi quy: tail cũ fail SAU khi làm việc. Install template-only
    // xong với Ok, không gọi adapter.)
    let dir = tempfile::tempdir().unwrap();
    install_at(dir.path(), vec![], None).await.unwrap();
}

#[tokio::test]
async fn install_at_rejects_unknown_package_first() {
    // Unknown names fail before any filesystem or adapter interaction.
    // (Tên lạ fail trước mọi tương tác filesystem/adapter.)
    let dir = tempfile::tempdir().unwrap();
    let err = install_at(dir.path(), vec!["nonsense".to_string()], None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("unknown hardware package"),
        "unexpected error: {err}"
    );
    assert!(!dir.path().join("nonsense").exists());
}
