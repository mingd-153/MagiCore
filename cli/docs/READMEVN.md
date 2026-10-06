# CLI MagiCore (`mgc`)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Xây binary giao diện người dùng: định nghĩa command, context project/core, nhận diện lệnh bare, dispatch common/per-core, handler và tích hợp scaffold/bundler. CLI điều phối adapter/crate chung; không nên nhân đôi package-manager behavior thuộc các lớp đó.

## Sơ đồ source

- [`commands/definitions.rs`](../src/commands/definitions.rs)
- [`dispatch/mod.rs`](../src/dispatch/mod.rs)
- [`context.rs`](../src/context.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)

## Bàn giao và review

Khi sửa command, trace khai báo → dispatch → handler → API shared/core. Giữ lệnh bare trung lập core; kiểm tra workspace/multi-core, không chỉ alias web.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
