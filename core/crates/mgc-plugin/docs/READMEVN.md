# mgc-plugin

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Định nghĩa contract plugin resolver/fetcher/linker/template/cache backend cùng cầu nối quanh `PackageAdapter` hiện có. Cầu nối hỗ trợ migration dần; có trait không chứng minh production đã đăng ký hay sử dụng.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`resolver.rs`](../src/resolver.rs)
- [`fetcher.rs`](../src/fetcher.rs)
- [`linker.rs`](../src/linker.rs)
- [`template.rs`](../src/template.rs)

## Bàn giao và review

Review tương thích API/object safety. Trace chỗ tạo registry và caller production trước khi claim plugin hoạt động.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-plugin`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
