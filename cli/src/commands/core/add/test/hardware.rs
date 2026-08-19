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
