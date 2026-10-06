# mgc-crypto

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp BLAKE3, checksum, ký/xác minh Ed25519, kiểu integrity, lưu keyring và nhận diện SIMD. Crate cung cấp primitive; policy tin key/identity/artifact thuộc lớp sử dụng.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`ed25519_signer.rs`](../src/ed25519_signer.rs)
- [`keyring.rs`](../src/keyring.rs)
- [`integrity.rs`](../src/integrity.rs)
- [`blake3_signer.rs`](../src/blake3_signer.rs)

## Bàn giao và review

Giữ kiểm tra format, quyền file key và lỗi integrity mismatch. Tách tính hợp lệ mật mã khỏi cấp trust.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-crypto`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
