# Adapter Game (mgc-game-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Nhận diện Bevy, Godot, Unity và Unreal; tích hợp scaffold engine, audit, helper phát triển và một số code install/cache. Adapter không claim resolver/lockfile tổng quát do MagiCore sở hữu cho mọi engine; kiểm tra engine cụ thể trước khi sửa thao tác package.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`engine.rs`](../src/engine.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)
- [`dev/mod.rs`](../src/dev/mod.rs)

## Bàn giao và review

Nhận diện engine và starter được sinh không chứng minh có quản lý dependency native. Giữ cổng capability và fail-closed với engine chưa hỗ trợ.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-game-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
