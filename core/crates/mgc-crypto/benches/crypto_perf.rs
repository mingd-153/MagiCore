//! Performance benchmarks for crypto operations
//! Benchmark hiệu năng cho các thao tác crypto

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mgc_crypto::blake3_signer::Blake3Hasher;
use mgc_crypto::ed25519_signer::{verify_signature, Ed25519Signer};
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;

fn bench_blake3_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_hash");

    for size in [1024, 10_240, 102_400, 1_024_000].iter() {
        let data = vec![0u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| Blake3Hasher::hash_bytes(black_box(&data)));
        });
    }

    group.finish();
}

fn bench_ed25519_sign(c: &mut Criterion) {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let message = b"hello world message for signing";

    c.bench_function("ed25519_sign", |b| {
        b.iter(|| signer.sign(black_box(message)));
    });
}

fn bench_ed25519_verify(c: &mut Criterion) {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let message = b"hello world message for verification";
    let signature = signer.sign(message);
    let public_key = signer.public_key();

    c.bench_function("ed25519_verify", |b| {
        b.iter(|| {
            verify_signature(
                black_box(&public_key),
                black_box(message),
                black_box(&signature),
            )
            .unwrap()
        });
    });
}

criterion_group!(
    benches,
    bench_blake3_hash,
    bench_ed25519_sign,
    bench_ed25519_verify
);
criterion_main!(benches);
