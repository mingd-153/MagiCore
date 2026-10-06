# mgc-http

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp thao tác HTTP outbound chung: TLS, timeout, retry, proxy, offline, rate limit, cache, telemetry và upload. Client registry/artifact nên dùng boundary này để behavior mạng nhất quán và có thể cấu hình/review.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`methods.rs`](../src/methods.rs)
- [`retry.rs`](../src/retry.rs)
- [`tls.rs`](../src/tls.rs)
- [`upload.rs`](../src/upload.rs)

## Bàn giao và review

Đổi mạng ảnh hưởng mọi caller. Kiểm tra retry/idempotency, fallback cache/offline, TLS và telemetry an toàn với secret; ngăn bypass bằng HTTP raw.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-http`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
