# mgc-cache

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Triển khai package cache có kiểm tra integrity khi đọc, lưu/vô hiệu hóa/prune, helper batch song song và string pool trong bộ nhớ. Crate khác durable content-addressable store `mgc-store`; chọn abstraction theo lifecycle/quyền sở hữu của caller.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`memory.rs`](../src/memory.rs)

## Bàn giao và review

Kiểm tra identity cache, digest mong đợi và prune cùng nhau. Cache hit không đáng tin nếu chưa so integrity; tránh nhân đôi trách nhiệm CAS của `mgc-store`.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-cache`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
