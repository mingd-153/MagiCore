use super::*;

#[test]
fn hardware_kind_accepts_optimizer_and_bench() {
    assert!(hardware_kind(OPTIMIZER_PKG).is_ok());
    assert!(hardware_kind(BENCH_PKG).is_ok());
}

#[test]
fn hardware_kind_rejects_unknown_package() {
    let err = hardware_kind("nonsense").unwrap_err();
    assert!(err.to_string().contains("unknown hardware package"));
}

#[tokio::test]
async fn add_with_version_pin_fails_loudly() {
    // T0.3-version: templates have no versions — the pin must fail
    // loudly, never drop silently (checked before any filesystem
    // access, so no fixture is needed).
    let err = add(
        vec!["optimizer".to_string()],
        None,
        Some("1.0.0".to_string()),
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("--version"),
        "unexpected error: {err}"
    );
}
