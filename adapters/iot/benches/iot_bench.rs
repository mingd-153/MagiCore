#![allow(clippy::unwrap_used)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mg_iot_adapter::{detect_framework, IotFramework};
use std::fs;
use tempfile::tempdir;

fn bench_iot_framework_detection(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("platformio.ini"),
        r#"[env:esp32dev]
platform = espressif32
board = esp32dev
framework = arduino
"#,
    )
    .unwrap();

    c.bench_function("iot_detect_framework_platformio", |b| {
        b.iter(|| {
            let fw = detect_framework(black_box(root));
            assert_eq!(fw, Some(IotFramework::Platformio));
        });
    });
}

fn bench_esp32_partition_table_parse(c: &mut Criterion) {
    // Binary partition table chuẩn của ESP32 (0x1000 bytes)
    let mut partition_data = vec![0u8; 3072];
    // Ghi magic byte 0xAA50
    partition_data[0] = 0xAA;
    partition_data[1] = 0x50;

    c.bench_function("iot_parse_esp32_partition_table", |b| {
        b.iter(|| {
            let is_valid =
                partition_data.len() >= 2 && partition_data[0] == 0xAA && partition_data[1] == 0x50;
            black_box(is_valid);
        });
    });
}

criterion_group!(
    benches,
    bench_iot_framework_detection,
    bench_esp32_partition_table_parse
);
criterion_main!(benches);
