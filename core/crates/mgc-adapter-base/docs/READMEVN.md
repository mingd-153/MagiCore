# mgc-adapter-base

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp default `BaseAdapter` và helper file project có kiểm tra. Crate parse/ghi Cargo manifest, đọc file thường và ghi file nguyên tử; đây là hạ tầng adapter, không phải resolver ecosystem.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`cargo_manifest.rs`](../src/cargo_manifest.rs)
- [`project_file.rs`](../src/project_file.rs)

## Bàn giao và review

Default ảnh hưởng mọi nơi dùng. Kiểm tra implementor, symlink/file đặc biệt, thay thế nguyên tử và truyền lỗi trước khi sửa behavior chung.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-adapter-base`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
