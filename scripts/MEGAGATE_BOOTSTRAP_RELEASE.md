# MegaGate Bootstrap Release

Ngày cập nhật: 2026-07-12

## Mục tiêu vòng này

Xây lớp phát hành tối thiểu nhưng thật cho 2 package đầu tiên:

- `megagate`
- `megagate-web`

để sau đó nhân mô hình này sang `ai`, `game`, `clo`, `cicd`, `iot`, `app`, `lib`.

## Những gì đã có

### 1. Package manifest

Bootstrap manifests chính đặt ở:

- [packaging/packages/megagate.toml](/Users/doanmihh/Documents/Workspace/MegaGate/packaging/packages/megagate.toml:1)
- [packaging/packages/megagate-web.toml](/Users/doanmihh/Documents/Workspace/MegaGate/packaging/packages/megagate-web.toml:1)

Ngoài 2 manifest bootstrap chính, repo giờ đã có sẵn manifest cho:

- `megagate-ai`
- `megagate-game`
- `megagate-clo`
- `megagate-cicd`
- `megagate-iot`
- `megagate-app`
- `megagate-lib`

Mỗi manifest định nghĩa:

- package name
- binary name
- build mode
- primary core
- cargo feature set
- install hint

### 2. Distribution builder

Đặt ở:

- [tools/mg-dist/src/main.rs](/Users/doanmihh/Documents/Workspace/MegaGate/tools/mg-dist/src/main.rs:1)

Command hỗ trợ:

```bash
cargo run -p mg-dist -- list
cargo run -p mg-dist -- build megagate
cargo run -p mg-dist -- build megagate-web
cargo run -p mg-dist -- build-bootstrap
cargo run -p mg-dist -- build-all
```

Output mặc định:

- `dist/megagate/<target>/mg`
- `dist/megagate-web/<target>/mg`
- kèm `build-receipt.json`

### 3. Script wrapper

Đặt ở:

- [scripts/build.sh](/Users/doanmihh/Documents/Workspace/MegaGate/scripts/build.sh:1)
- [scripts/release.sh](/Users/doanmihh/Documents/Workspace/MegaGate/scripts/release.sh:1)

Ví dụ:

```bash
./scripts/build.sh megagate
./scripts/build.sh megagate-web
./scripts/build.sh bootstrap
./scripts/build.sh megagate-ai
./scripts/build.sh all
./scripts/release.sh
```

## Build shape hiện tại

### `megagate`

- build kiểu multi-core
- cargo features: `all`
- binary output: `mg`

### `megagate-web`

- build kiểu single-core web
- cargo features: `web`
- binary output: `mg`

## Lưu ý kỹ thuật

Đây là bootstrap layer cho local build/distribution, chưa phải publish layer hoàn chỉnh.

Nó chưa làm các việc sau:

- tạo Homebrew formula
- ký artifact
- tạo checksum
- publish GitHub Release
- publish installer cho Windows/Linux/macOS

## Bước tiếp theo hợp lý

Hiện lớp manifest/build local đã mở rộng ra toàn bộ core package.

Bước tiếp theo là:

- nối release workflow đủ sâu cho toàn bộ package matrix
- thêm checksum / signing / release notes
- thêm Homebrew formula và installer thật theo từng nền tảng
