# mgc-ui

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp primitive giao diện terminal: help, progress bar/spinner, prompt, table, output trạng thái/thông tin và quiet mode. Crate sở hữu presentation dùng lại, không sở hữu policy command/nghiệp vụ.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`help.rs`](../src/help.rs)
- [`progress.rs`](../src/progress.rs)
- [`prompt.rs`](../src/prompt.rs)
- [`table.rs`](../src/table.rs)

## Bàn giao và review

Giữ output terminal tiếng Anh theo RULE.md; bảo toàn quiet/non-TTY, tránh báo thành công cũ/gây hiểu nhầm khi lỗi.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-ui`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
