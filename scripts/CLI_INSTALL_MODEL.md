# MegaGate CLI Install Model

Ngày cập nhật: 2026-07-11

## 1. Mô hình hiện tại

Repo hiện tại đang đi theo hướng OP2.

Nghĩa là:

- `megagate` = full build / multi-core build
- `megagate-web` = single-core web build
- `megagate-ai` = single-core ai build
- `megagate-game`, `megagate-app`, `megagate-clo`, `megagate-cicd`, `megagate-iot`, `megagate-lib` tương tự

Ở mức code hiện tại, binary `mg` đang chạy là nguồn sự thật để quyết định command surface.

Không có runtime detector kiểu:

- quét toàn máy
- đọc registry hệ điều hành
- tìm xem đã cài bao nhiêu core package

Nên “single-core hay multi-core” hiện được xác định bằng chính build shape của binary.

## 2. Cách hiểu đúng

### Single-core

Ví dụ build web-only:

- command surface:
  - `mg create ...`
  - `mg add ...`
  - `mg remove ...`
  - `mg list`
  - `mg update ...`
  - `mg install ...`
- help hiển thị:
  - `Build: single-core (web)`

### Multi-core

Ví dụ full build:

- command surface:
  - `mg create-web ...`
  - `mg add-web ...`
  - `mg remove-web ...`
  - `mg list-web`
  - `mg update-web ...`
  - `mg install-web ...`
  - tương tự cho `game`, `ai`, `clo`, `cicd`, `iot`, `app`, `lib`
- help hiển thị:
  - `Build: multi-core (web, game, ai, clo, cicd, iot, app, lib)`

## 2.1. Packaging state hiện tại

Manifest build local hiện đã có cho:

- `megagate`
- `megagate-web`
- `megagate-ai`
- `megagate-game`
- `megagate-clo`
- `megagate-cicd`
- `megagate-iot`
- `megagate-app`
- `megagate-lib`

## 3. Điều đã fix trong vòng này

- help đã hiện build profile rõ ràng
- docs đã nói rõ repo đang theo OP2
- `DESIGN_FLOW` đã sửa lại từ 7 core thành 8 core
- `iot` đã được đưa lại đúng vào danh sách core cài đặt

## 4. Điều chưa có

Hiện repo chưa có:

- Homebrew formula thật
- Windows installer thật
- Linux package/install script publish thật
- runtime machine-wide installed-core registry

Nghĩa là behavior CLI và local distribution build đã đúng theo shape mong muốn, nhưng packaging/distribution ngoài đời vẫn là bước tiếp theo.

## 5. Kết luận kỹ thuật

Nếu giữ OP2 thì cách đúng để trả lời câu hỏi:

> “máy này đang cài core gì?”

ở thời điểm hiện tại là:

- xem binary `mg` đang chạy là build nào
- xem help/build profile của chính binary đó

chứ chưa phải:

- scan toàn hệ thống để hợp nhất nhiều package core đã cài.
