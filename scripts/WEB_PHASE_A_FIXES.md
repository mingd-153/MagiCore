# Web Phase A Fixes

Ngày cập nhật: 2026-07-11

## Mục tiêu vòng này

Khóa nốt các điểm còn hở sau phase A web:

- cache metadata phải giữ được `ETag` thật
- install không được chết cứng khi hardlink không khả dụng
- scaffold không được rơi về version `"*"`
- single-core và multi-core phải tách rõ ở CLI surface
- help và docs phải phản ánh đúng hành vi thực tế

## Những gì đã fix

### 1. Root detection thống nhất hơn

Files:

- `core/crates/mg-config/src/project.rs`
- `cli/src/commands/core/shared.rs`

Thay đổi:

- dùng chung `ProjectConfig::find_project_root(...)`
- chỉ nhận parent project khi có `.megagate/project.toml`
- không leo theo `package.json` ở parent để tránh match nhầm workspace hoặc home directory

Tác động:

- `mg add`, `mg install`, `mg list` bớt lệch hành vi giữa các command
- giảm false-positive khi đang đứng trong thư mục con hoặc môi trường có file cấu hình ngoài ý muốn

### 2. Metadata cache giữ được ETag

Files:

- `adapters/web/src/native/npm_registry.rs`
- `adapters/web/src/lib.rs`

Thay đổi:

- thêm `fetch_metadata_with_etag(...)`
- lần fetch đầu tiên lưu cả metadata và `ETag`
- về sau stale cache có thể đi qua nhánh conditional request thật

Tác động:

- freshness policy đúng hơn với thiết kế đã mô tả
- giảm khả năng fetch toàn bộ metadata lặp lại không cần thiết

### 3. Hardlink materialization có fallback

File:

- `adapters/web/src/lib.rs`

Thay đổi:

- `hardlink_tree(...)` hardlink trước
- nếu hardlink fail thì fallback sang copy cho file đó

Tác động:

- install bền hơn trên filesystem không hỗ trợ hardlink tốt
- vẫn giữ được fast path khi hardlink khả dụng

### 4. Scaffold không seed `*`

File:

- `cli/src/commands/core/web.rs`

Thay đổi:

- bỏ fallback `"*"` khi fetch latest version lỗi
- convert luồng resolve version sang trả lỗi thật

Tác động:

- output project ổn định hơn
- tránh sinh manifest “chạy được nhưng khó audit / khó reproduce”

### 5. Help surface và docs khớp build shape

Files:

- `core/crates/mg-ui/src/help.rs`
- `adapters/web/README.md`

Thay đổi:

- help custom giờ render theo build shape thật
- single-core chỉ hiện bare commands
- multi-core hiện `create-web`, `add-web`, `install-web`
- docs web adapter ghi rõ sự khác nhau giữa single-core và multi-core

Tác động:

- giảm nhầm lẫn cho user khi đọc help
- giảm sai lệch giữa CLI thật và tài liệu

### 6. Dọn artifact thừa

File:

- xóa `cli/src/main.rs.bak`

Tác động:

- giảm nhiễu khi review
- tránh hiểu nhầm giữa code đang chạy và file backup cũ

### 7. `create-web` có curated scaffold baseline cho cold/offline path

File:

- `cli/src/commands/core/web.rs`

Thay đổi:

- thêm `SCAFFOLD_BASELINE_VERSIONS` cho nhóm package scaffold cốt lõi
- `fetch_npm_latest_version(...)` giờ ưu tiên:
  - env override
  - registry `latest`
  - curated baseline nội bộ nếu registry lỗi
- giữ nguyên nguyên tắc không rơi về `"*"`

Tác động:

- `mg create-web` bền hơn khi offline hoặc mạng chập chờn
- React / Next / Fastify / monorepo web path không còn fail sớm chỉ vì không lấy được `latest`
- output vẫn reproducible hơn wildcard fallback cũ

## Test đã chạy

- `cargo test -p mg-config`
- `cargo test -p mg-web-adapter --lib`
- `cargo test -p mg test_create_web_with_flags_seeds_package_json -- --nocapture`
- `cargo test -p mg test_fetch_npm_latest_version_from_registry_errors_without_version_field -- --nocapture`
- `cargo test -p mg test_scaffold_baseline_version_covers_core_web_seed_packages -- --nocapture`
- `cargo test -p mg test_create_web_without_overrides_uses_curated_baseline_when_registry_is_unavailable -- --nocapture`

## Đánh giá hiện trạng

Sau vòng này, phase A web ổn hơn ở ba lớp:

- runtime behavior: cache/install/scaffold đúng hơn
- CLI behavior: single-core vs multi-core rõ hơn
- maintenance surface: help/docs/artefact sạch hơn

Nhưng vẫn còn việc nên làm tiếp:

- tách `adapters/web/src/lib.rs` thành nhiều module nhỏ
- audit thêm `create/add/install` của các core khác cho đồng đều với web
- cập nhật lại `REPORT.md` nếu muốn dùng nó như báo cáo canonical cuối vòng
