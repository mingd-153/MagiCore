# Adapter IoT (mgc-iot-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Nhận diện ESP32 Rust, PlatformIO và Zephyr; cung cấp metadata board/target, scaffold, helper install và tích hợp flash. Resolve dependency tổng quát hiện chưa phải capability adapter; flash thiết bị thuộc ranh giới hardware/toolchain.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`framework.rs`](../src/framework.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)
- [`flash/mod.rs`](../src/flash/mod.rs)

## Bàn giao và review

Tách metadata target khỏi việc chạy hardware. Trace luồng dùng engine MagiCore hay ủy quyền toolchain ngoài; báo rõ trường hợp chưa hỗ trợ/thiếu thiết bị.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-iot-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
