# mgc-search

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Điều phối client discovery npm, crates.io, Go và PyPI; chuẩn hóa kết quả, xếp hạng candidate, cache query và hỗ trợ prompt. Search result là dữ liệu discovery, không phải kế hoạch install đã resolve/được tin cậy.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`orchestrator.rs`](../src/orchestrator.rs)
- [`clients/mod.rs`](../src/clients/mod.rs)
- [`ranking.rs`](../src/ranking.rs)
- [`types.rs`](../src/types.rs)

## Bàn giao và review

Kiểm tra nguồn, pagination/rate limit, ranking và cache freshness. Resolve install/trust thuộc resolver/adapter; kết quả xếp hạng không phải thẩm quyền.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-search`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
