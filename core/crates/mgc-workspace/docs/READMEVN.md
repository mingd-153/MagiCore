# mgc-workspace

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Khám phá member/catalog, dựng quan hệ dependency, lọc project, cache computation và trả về các tầng topo. CLI lên lịch các tầng đó; crate mô hình hóa lựa chọn/thứ tự chứ không chạy command.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`discover.rs`](../src/discover.rs)
- [`graph.rs`](../src/graph.rs)
- [`filter.rs`](../src/filter.rs)
- [`topo.rs`](../src/topo.rs)

## Bàn giao và review

Kiểm tra ranh giới workspace, identity member, chiều cạnh, filter, thứ tự xác định và cách CLI xử lý lỗi một phần.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-workspace`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
