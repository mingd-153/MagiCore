# Adapter AI (mgc-ai-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Nối project AI với core, capability, audit, cache và install của MagiCore. Detector nhận nhãn `python-agent` và `mcp-server`; crate còn có tải/kiểm tra checksum model và client Hugging Face native. Điều này không có nghĩa hỗ trợ mọi ngôn ngữ AI hay runtime model.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`framework.rs`](../src/framework.rs)
- [`install/mod.rs`](../src/install/mod.rs)
- [`native/hf_client.rs`](../src/native/hf_client.rs)

## Bàn giao và review

Trace capability được chọn trước khi sửa dependency/runtime. Giữ từ chối rõ ràng và kiểm tra checksum; chỉ nhận diện framework không chứng minh đã có lane đầy đủ.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-ai-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
