# mgc-publish

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp lane publish client tới registry MagiCore, gồm phân giải auth và điều phối publish/retry. Crate không sở hữu tạo tarball, policy server hay command CLI; trace các lớp đó khi sửa đầu-cuối.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`auth.rs`](../src/auth.rs)

## Bàn giao và review

Trace nguồn/scope token, registry target, retry/idempotency và server chấp nhận. Helper auth không chứng minh OIDC/provenance đầu-cuối.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-publish`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
