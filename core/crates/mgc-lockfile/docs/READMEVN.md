# mgc-lockfile

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Sở hữu schema/chuyển đổi lockfile cho graph tái lập: parse/serialize canonical, đường v3/v4, migration, ecosystem tag, root pin, merge/import/export, ghi nguyên tử và xác minh signature/policy. Parse được lock không chứng minh đã đánh giá trust signer.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`canonical.rs`](../src/canonical.rs)
- [`v4.rs`](../src/v4.rs)
- [`verifier.rs`](../src/verifier.rs)
- [`writer.rs`](../src/writer.rs)

## Bàn giao và review

Review schema, byte canonical, digest/signature, trust signer, owner/core tag và ghi nguyên tử ở cả producer/consumer. Giữ phân biệt absent/malformed/lossy/unsigned/invalid.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-lockfile`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
