# MegaGate — Web/Core Status Report

Ngày cập nhật: 2026-07-11

## 1. Trạng thái hiện tại

Repo hiện tại đã đi theo hướng:

- mỗi core có command surface riêng trong `cli/src/commands/core/`
- web là core đang triển khai sâu nhất
- adapter web đã được tách riêng để không nhồi chung logic với các core khác
- single-core build và multi-core build đã tách command surface khác nhau
- binary shape hiện đóng vai trò “nguồn sự thật” cho core đang có trong bản cài

## 1.1. Mô hình cài đặt hiện tại

Hiện tại hướng phù hợp với code là OP2:

- `brew install megagate` -> cài bản `mg` full build, chứa nhiều core
- `brew install megagate-web` -> cài bản `mg` single-core web
- `brew install megagate-ai` -> cài bản `mg` single-core ai
- `brew install megagate-game|megagate-clo|megagate-cicd|megagate-iot|megagate-app|megagate-lib` -> các single-core build tương ứng

Điểm quan trọng:

- repo chưa có runtime detector kiểu “quét toàn máy xem đã cài core gì”
- thay vào đó, chính binary `mg` được build với feature nào thì sẽ lộ ra command surface tương ứng
- vì vậy, cách kiểm tra “máy này đang có core gì” ở thời điểm hiện tại là xem build profile của binary đang chạy

Sau vòng sửa này, help output sẽ hiện rõ:

- `Build: single-core (web)`
- hoặc `Build: multi-core (web, game, ai, ...)`

## 2. Cấu trúc per-core command

Hiện tại mỗi core đã có file command riêng:

- `cli/src/commands/core/web.rs`
- `cli/src/commands/core/game.rs`
- `cli/src/commands/core/ai.rs`
- `cli/src/commands/core/clo.rs`
- `cli/src/commands/core/cicd.rs`
- `cli/src/commands/core/iot.rs`
- `cli/src/commands/core/app.rs`
- `cli/src/commands/core/library.rs`

`cli/src/commands/core/mod.rs` chỉ đóng vai trò registry module:

- export từng core
- giữ `shared.rs` cho logic dùng chung

Điểm quan trọng:

- mỗi core đã có `add/remove/list/update/install` riêng ở cấp file/module
- web hiện đang gọi sâu vào adapter thật
- các core còn lại đang ở trạng thái stub riêng biệt, chưa share implementation runtime với web

Điều này đúng với mục tiêu “sau này mỗi core có thể tùy biến riêng”.

## 3. Web đã sửa gì

### CLI/scaffold

- `create` và `create-web` được tách theo build shape:
  - single-core web: `mg create ...`
  - multi-core: `mg create-web ...`
- `add` và `add-web` hỗ trợ nhiều package cùng lúc
- web scaffold dùng `FRAMEWORK_SEEDS` để seed package theo framework
- framework/toolchain version được resolve động
- test scaffold không còn phụ thuộc network cứng như trước

### Packaging/bootstrap distribution

- đã có package manifest riêng cho:
  - `megagate`
  - `megagate-web`
  - `megagate-ai`
  - `megagate-game`
  - `megagate-clo`
  - `megagate-cicd`
  - `megagate-iot`
  - `megagate-app`
  - `megagate-lib`
- đã có tool build phân phối cục bộ:
  - `cargo run -p mg-dist -- build megagate`
  - `cargo run -p mg-dist -- build megagate-web`
  - `cargo run -p mg-dist -- build <package>`
  - `cargo run -p mg-dist -- build-all`
- đã có wrapper script:
  - `./scripts/build.sh megagate`
  - `./scripts/build.sh megagate-web`
  - `./scripts/build.sh bootstrap`
  - `./scripts/build.sh all`
- output thật hiện được materialize vào:
  - `dist/megagate/<target>/mg`
  - `dist/megagate-web/<target>/mg`
  - `dist/megagate-<core>/<target>/mg`
- mỗi output có `build-receipt.json` đi kèm để ghi lại build shape / feature set / target

### Adapter/runtime

- metadata cache có giữ `ETag`
- metadata stale path có thể dùng conditional request
- tarball integrity được verify theo SRI
- install path dùng cache/shared cache tốt hơn
- materialization hardlink có fallback sang copy
- extracted package cache key không còn hash truncation kiểu cũ

### Root detection

- project root chỉ coi là “parent project” khi có `.megagate/project.toml`
- không còn leo parent `package.json` để tránh detect nhầm

### Help/docs

- help custom giờ phân biệt đúng single-core và multi-core
- `adapters/web/README.md` đã cập nhật lại surface thật
- `cli/src/main.rs.bak` đã được dọn

## 4. Test đã pass

### Đã verify trực tiếp

- `cargo test -p mg-ui`
- `cargo test -p mg-config`
- `cargo test -p mg -- --nocapture`
- `cargo test -p mg-web-adapter --lib`

### Kết quả hiện tại

- `mg-ui`: pass
- `mg-config`: pass
- `mg` CLI: pass `16/16`
- `mg-web-adapter`: pass `24/24`

## 5. Build shape hiện tại

### Single-core web build

Surface hiện tại:

- `mg create <framework> <project>`
- `mg add <packages...>`
- `mg remove <package>`
- `mg list`
- `mg update [packages...]`
- `mg install [packages...]`

### Multi-core build

Surface hiện tại cho web:

- `mg create-web <framework> <project>`
- `mg add-web <packages...>`
- `mg remove-web <package>`
- `mg list-web`
- `mg update-web [packages...]`
- `mg install-web [packages...]`

## 6. So với mục tiêu kiến trúc

### Đã đúng

- mỗi core có module command riêng
- web adapter đã tách riêng
- command surface đã split theo single-core và multi-core
- web có đường phát triển riêng, không ép chung vào các core còn lại
- mô hình cài đặt kiểu OP2 giờ đã được nói rõ hơn trong code/docs: binary shape quyết định core surface

### Chưa xong

- các core ngoài web mới ở mức stub command, chưa có adapter/runtime thật
- `cli/src/main.rs` còn dài và đang ôm nhiều enum command
- `adapters/web/src/lib.rs` vẫn quá lớn, cần tách module
- chưa có launcher/runtime detector toàn cục kiểu “một mg biết nhiều package core đang nằm trên máy”
- runtime thật vẫn mới sâu ở web; package manifest/build matrix đã mở cho toàn bộ core còn lại

## 7. Kết luận

Hiện tại nền CLI/core đã chuyển được sang mô hình:

- mỗi core có entry command riêng
- web phát triển độc lập hơn
- shared chỉ còn là lớp helper, không còn là nơi “nuốt” luôn identity của từng core

Đây là hướng đúng để sau này mỗi core tự có:

- add
- remove
- list
- update
- install
- create

theo behavior riêng của nó.

Nhưng nếu giữ OP2 thì cần chấp nhận một điều:

- mỗi package cài đặt (`megagate`, `megagate-web`, `megagate-ai`, ...) sẽ quyết định shape của `mg`
- “single hay 2+ core” hiện không phải do scan hệ thống, mà do binary mà user đang chạy
