// ponytail: coarse scaffold speed benchmarks, tighten thresholds when infra improves.

mod common;

#[test]
fn bench_react_scaffold() {
    let ms = common::bench_scaffold("react", "bench-react-ts");
    assert!(ms < 40000, "react scaffold took {ms}ms (limit 40000)");
}

#[test]
fn bench_vue_scaffold() {
    let ms = common::bench_scaffold("vue", "bench-vue-ts");
    assert!(ms < 40000, "vue scaffold took {ms}ms (limit 40000)");
}

#[test]
fn bench_express_scaffold() {
    let ms = common::bench_scaffold("express", "bench-express");
    assert!(ms < 40000, "express scaffold took {ms}ms (limit 40000)");
}

#[test]
fn bench_fastapi_scaffold() {
    let ms = common::bench_scaffold("fastapi", "bench-fastapi");
    assert!(ms < 40000, "fastapi scaffold took {ms}ms (limit 40000)");
}
