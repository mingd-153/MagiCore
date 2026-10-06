# mgc-exec

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Tập trung passthrough process ngoài qua allowlist executable/argument, wrapper process, record audit và che secret. Đây là boundary policy tiến trình, không phải API shell tùy ý.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`allowlist.rs`](../src/allowlist.rs)
- [`run.rs`](../src/run.rs)
- [`audit.rs`](../src/audit.rs)
- [`sanitizer.rs`](../src/sanitizer.rs)

## Bàn giao và review

Coi sửa allowlist/sanitizer là nhạy cảm bảo mật. Giữ mặc định từ chối; xác minh đối số và record audit vẫn được che khi lỗi.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-exec`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
