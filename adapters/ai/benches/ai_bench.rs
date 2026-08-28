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

fn bench_ai_token_pruning_simulation(c: &mut Criterion) {
    // Mô phỏng đo tốc độ xử lý lọc token mask (32.768 context length)
    let context_tokens: Vec<u32> = (0..32768).collect();
    let attention_mask: Vec<bool> = (0..32768).map(|i| i % 4 == 0).collect();

    c.bench_function("ai_sparse_token_pruning_32k", |b| {
        b.iter(|| {
            let active: Vec<u32> = context_tokens
                .iter()
                .zip(attention_mask.iter())
                .filter_map(|(&tok, &active)| if active { Some(tok) } else { None })
                .collect();
            black_box(active);
        });
    });
}

criterion_group!(
    benches,
    bench_ai_framework_detection,
    bench_ai_token_pruning_simulation
);
criterion_main!(benches);
