use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mg_app_adapter::{detect_language, AppLanguage};
use std::fs;
use tempfile::tempdir;

fn bench_app_language_detection(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("pubspec.yaml"),
        r#"name: demo_app
description: A new Flutter project.
version: 1.0.0+1
environment:
  sdk: ">=3.0.0 <4.0.0"
"#,
    )
    .unwrap();

    c.bench_function("app_detect_language_flutter_pubspec", |b| {
        b.iter(|| {
            let lang = detect_language(black_box(root));
            assert_eq!(lang, Some(AppLanguage::Flutter));
        });
    });
}

fn bench_app_multi_platform_parsing(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("mg.toml"),
        r#"[app]
language = "multi"
platforms = ["ios", "android"]

[app.ios]
framework = "swift"

[app.android]
framework = "kotlin"
"#,
    )
    .unwrap();

    c.bench_function("app_detect_language_multi_platform_mg_toml", |b| {
        b.iter(|| {
            let lang = detect_language(black_box(root));
            assert_eq!(lang, Some(AppLanguage::Multi));
        });
    });
}

criterion_group!(
    benches,
    bench_app_language_detection,
    bench_app_multi_platform_parsing
);
criterion_main!(benches);
