use super::*;

#[test]
fn remove_rejects_unknown_package_before_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let error = runtime
        .block_on(remove_at(dir.path(), vec!["nonsense".to_string()], None))
        .unwrap_err();
    assert!(
        error.to_string().contains("unknown hardware package"),
        "{error:#}"
    );
}

#[tokio::test]
async fn remove_absent_template_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let error = remove_at(dir.path(), vec![OPTIMIZER_PKG.to_string()], None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("nothing to remove"), "{error:#}");
}

#[tokio::test]
async fn remove_present_template_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(BENCH_PKG)).unwrap();
    std::fs::write(dir.path().join(BENCH_PKG).join("bench.json"), "{}\n").unwrap();
    remove_at(dir.path(), vec![BENCH_PKG.to_string()], None)
        .await
        .unwrap();
    assert!(!dir.path().join(BENCH_PKG).exists());
}
