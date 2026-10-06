# mgc-pack

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Tạo tarball package với rule chọn/ignore file, sanitize manifest, đóng archive theo hướng streaming và content hash. Chọn registry, xác thực và quyền publish thuộc lớp publish.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`ignore.rs`](../src/ignore.rs)
- [`manifest.rs`](../src/manifest.rs)
- [`tarball.rs`](../src/tarball.rs)

## Bàn giao và review

Audit include/exclude, path, khả năng tái lập và hash. Digest chứng minh byte identity, không chứng minh package đáng tin.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-pack`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
