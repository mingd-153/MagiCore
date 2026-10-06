# mgc-platform

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp API filesystem đa nền tảng tầng thấp hiện tại: path chuẩn, semaphore giới hạn ghi và reflink. Danh sách module public hẹp hơn một số mô tả lịch sử; không giả định API shell/permission/symlink khi chưa xem source.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`paths.rs`](../src/paths.rs)
- [`fs_semaphore.rs`](../src/fs_semaphore.rs)
- [`reflink.rs`](../src/reflink.rs)

## Bàn giao và review

Giữ fallback tường minh khi reflink không có. Xác minh path theo OS và bảo đảm thao tác ghi mới tuân thủ giới hạn đồng thời.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-platform`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
