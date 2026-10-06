# mgc-oci

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Triển khai OCI Distribution: liệt kê catalog/tag, pull/push blob/manifest, cấu hình bearer token và xác minh digest blob tải về. Crate cung cấp transport; identity package/model và phân quyền tầng cao thuộc caller.

## Sơ đồ source

- [`client.rs`](../src/client.rs)
- [`manifest.rs`](../src/manifest.rs)
- [`ref.rs`](../src/ref.rs)

## Bàn giao và review

Kiểm tra dựng reference/URL, truyền auth, status, digest và upload. OCI transport không chứng minh trusted publishing/provenance all-core.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-oci`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
