# mgc-store

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Triển khai package store bền vững dựa trên CAS, integrity, lifecycle/cache, database/index và điều phối install/write. Crate có generation/lease state, hook chèn lỗi và khác `mgc-cache` nhẹ hơn.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`cas/mod.rs`](../src/cas/mod.rs)
- [`database.rs`](../src/database.rs)
- [`index.rs`](../src/index.rs)
- [`layout.rs`](../src/layout.rs)

## Bàn giao và review

Review crash consistency, ghi đồng thời, phục hồi lease, reference, integrity và migration. Không biến lỗi xác minh thành miss hay tự sửa im lặng khi thiếu căn cứ.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-store`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
