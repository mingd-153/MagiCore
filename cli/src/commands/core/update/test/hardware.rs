use super::*;

#[test]
fn update_rejects_unknown_package_before_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let error = runtime
        .block_on(update_at(
            dir.path(),
            vec!["nonsense".to_string()],
            false,
            None,
        ))
        .unwrap_err();
    assert!(
        error.to_string().contains("unknown hardware package"),
        "{error:#}"
    );
}

#[tokio::test]
async fn update_materializes_missing_template() {
    let dir = tempfile::tempdir().unwrap();
    update_at(dir.path(), vec![BENCH_PKG.to_string()], false, None)
        .await
        .unwrap();
    assert!(dir.path().join(BENCH_PKG).is_dir());
}

#[tokio::test]
async fn update_refreshes_existing_template_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    update_at(dir.path(), vec![OPTIMIZER_PKG.to_string()], true, None)
        .await
        .unwrap();
    assert!(dir.path().join(OPTIMIZER_PKG).is_dir());
    // Second run refreshes in place (remove + re-materialize).
    update_at(dir.path(), vec![OPTIMIZER_PKG.to_string()], false, None)
        .await
        .unwrap();
    assert!(dir.path().join(OPTIMIZER_PKG).is_dir());
}
