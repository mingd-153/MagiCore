# Adapter ứng dụng di động (mgc-app-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Nhận diện Flutter, Kotlin, Swift, React Native, Objective-C và project đa nền tảng; điều phối parse/ghi manifest cùng helper audit/cache/install. Resolver tùy ngôn ngữ; nhận diện được ngôn ngữ không đồng nghĩa đã hỗ trợ trọn vòng đời dependency.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`language.rs`](../src/language.rs)
- [`manifest/mod.rs`](../src/manifest/mod.rs)
- [`native/mod.rs`](../src/native/mod.rs)

## Bàn giao và review

Trace từng ngôn ngữ qua nhận diện, quyền sở hữu manifest, capability và dispatch CLI. Chỉ có parser/marker không chứng minh install được hỗ trợ.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-app-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
