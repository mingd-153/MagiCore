#![allow(clippy::unwrap_used)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mgc_ai_adapter::{detect_framework, AiFramework};
use std::fs;
use tempfile::tempdir;

fn bench_ai_framework_detection(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("pyproject.toml"),
        r#"[tool.magicore]
framework = "python-agent"
core = "ai"
"#,
    )
    .unwrap();

    c.bench_function("ai_detect_framework_pyproject", |b| {
        b.iter(|| {
            let fw = detect_framework(black_box(root));
            assert_eq!(fw, Some(AiFramework::PythonAgent));
        });
    });
}

criterion_group!(benches, bench_ai_framework_detection);
criterion_main!(benches);
