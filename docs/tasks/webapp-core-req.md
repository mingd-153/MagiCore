# Task: Xây dựng WebApp Core (Bun / PNPM) cho Hyper-Pkg

## 1. Mục tiêu
Xây dựng một **core (adapter) duy nhất** cho các dự án JavaScript/TypeScript, dựa trên **Bun** và **PNPM**, cung cấp:
- Quản lý phụ thuộc (`install`, `update`, `remove`) tự động chọn công cụ (Bun → PNPM → npm).
- Các chức năng bổ sung: Bundle/Build, Dev-server (HMR), Lint, Test, Type-checking.
- Ghi lại mọi dependency vào `MegaGate.lock` với `source = "webapp"`.
- Kiến trúc module hóa, dễ mở rộng (thêm Webpack, Rollup, Yarn...).

## 2. Phạm vi công việc (Scope)

### 2.1. Định nghĩa Adapter
- **File**: `src/adapters/webapp/mod.rs`
- **Nhiệm vụ**:
  - Tạo struct `WebAppCoreAdapter`.
  - Triển khai trait `Adapter` (parse, install, update, remove).
  - Đọc `package.json` và điền vào `LockFile`.
- **Đầu ra**: `WebAppCoreAdapter` hoạt động như các adapter khác.

### 2.2. Engine (Bun / PNPM / npm)
- **File**: `src/core/webapp_core.rs`
- **Nhiệm vụ**:
  - Định nghĩa enum `WebAppEngine` (Bun, Pnpm, Npm).
  - Cài đặt hàm `install()`, `update()`, `remove()` thực thi lệnh tương ứng.
  - Hàm chọn engine tự động (`pick_engine()`).
- **Đầu ra**: Engine có thể gọi lệnh cài đặt/cập nhật/gỡ bỏ.

### 2.3. TS Tooling Layer
- **File**: `src/core/ts_tool.rs`
- **Nhiệm vụ**:
  - Định nghĩa enum `TsTool` (Bun, Vite, Esbuild, Tsc).
  - Hàm `pick_best()` chọn công cụ tốt nhất dựa trên sự sẵn có.
  - Hàm `run_ts_tool(mode)` thực hiện `build`, `dev`, `lint`, `test`.
- **Đầu ra**: Bộ công cụ TS gọi được bằng một lệnh duy nhất.

### 2.4. Detect Logic
- **File**: `src/adapters/mod.rs`
- **Nhiệm vụ**:
  - Cập nhật hàm `detect()` để nhận diện `bun.lockb` hoặc `pnpm-lock.yaml`.
  - Trả về `WebAppCoreAdapter` nếu có lockfile, ngược lại fallback `NpmAdapter`.
  - Thêm `pub mod webapp;` để export module.
- **Đầu ra**: Hệ thống tự động chọn đúng adapter.

### 2.5. CLI Extension
- **File**: `src/commands/run_ts.rs` (mới) + `src/main.rs`
- **Nhiệm vụ**:
  - Thêm sub-command `run-ts <mode>` (build, dev, lint, test).
  - Kết nối command tới `WebAppCoreAdapter::run_ts_tool`.
- **Đầu ra**: Người dùng chạy được `MegaGate run-ts build`.

### 2.6. Tài liệu
- **File**: `docs/tasks/webapp-core-req.md` (file này) + cập nhật `CORE.md`.
- **Nhiệm vụ**:
  - Mô tả cấu trúc thư mục.
  - Liệt kê adapter và chức năng.
  - Hướng dẫn sử dụng và mở rộng.
- **Đầu ra**: Tài liệu hoàn chỉnh.

### 2.7. Kiểm thử
- **File**: `src/adapters/webapp/test.rs` (hoặc `#[cfg(test)]` trong mod.rs).
- **Nhiệm vụ**:
  - Test `detect` trả về đúng adapter.
  - Test `WebAppEngine` chọn đúng công cụ.
  - Mock `Command` để test `run_ts_tool`.
- **Đầu ra**: Unit test pass với `cargo test`.

### 2.8. CI/CD mẫu
- **File**: `.github/workflows/webapp-core-ci.yml`
- **Nhiệm vụ**:
  - Chạy `cargo test`.
  - Chạy `MegaGate install` và `run-ts build` trên Ubuntu, macOS, Windows.
- **Đầu ra**: Workflow CI hoạt động.

## 3. Yêu cầu phi chức năng

| Yêu cầu | Mô tả |
|---------|------|
| **Hiệu năng** | Ưu tiên Bun (nhanh nhất), sau đó PNPM, cuối cùng npm. |
| **Ổn định** | Mọi lỗi phải được bọc trong `anyhow::Result`, thông báo rõ ràng. |
| **Mở rộng** | Thêm công cụ mới chỉ cần sửa enum `TsTool`/`WebAppEngine`. |
| **Đồng nhất** | Mọi dependency đều ghi vào `MegaGate.lock` (JSON). |
| **Testable** | Sử dụng mock cho `std::process::Command`. |
| **Bảo mật** | Chỉ chạy các lệnh xác định rõ, không thực thi script lạ. |
| **Đa nền tảng** | Chạy trên macOS, Linux, Windows. |
| **Documentation** | Đầy đủ hướng dẫn sử dụng và mở rộng. |

## 4. Tiến độ (Timeline)

| Giai đoạn | Công việc | Thời gian | Trạng thái |
|-----------|-----------|-----------|------------|
| 1. Chuẩn bị | Tạo folder, file, cập nhật `Cargo.toml` | 0.5 ngày | ⬜ Chưa bắt đầu |
| 2. Engine | Viết `WebAppEngine` (install/update/remove) | 1 ngày | ⬜ Chưa bắt đầu |
| 3. Adapter | Viết `WebAppCoreAdapter` (parse, install, update, remove) | 1 ngày | ⬜ Chưa bắt đầu |
| 4. TS Tooling | Viết `TsTool`, `pick_best`, `run_ts_tool` | 2 ngày | ⬜ Chưa bắt đầu |
| 5. Detect | Cập nhật `detect` trong `adapters/mod.rs` | 0.5 ngày | ⬜ Chưa bắt đầu |
| 6. CLI | Thêm command `run-ts` | 0.5 ngày | ⬜ Chưa bắt đầu |
| 7. Tài liệu | Cập nhật `CORE.md` | 0.5 ngày | ⬜ Chưa bắt đầu |
| 8. Test | Viết unit test | 1 ngày | ⬜ Chưa bắt đầu |
| 9. CI/CD | Tạo workflow GitHub Actions | 1 ngày | ⬜ Chưa bắt đầu |
| **Tổng cộng** | | **8 ngày** | |

## 5. Cấu trúc thư mục dự kiến

```
MegaGate/
├─ src/
│  ├─ adapters/
│  │   ├─ webapp/
│  │   │   └─ mod.rs          ← WebAppCoreAdapter
│  │   ├─ mod.rs              ← Cập nhật detect + pub mod webapp
│  │   └─ ... (cargo, npm, ...)
│  ├─ core/
│  │   ├─ webapp_core.rs      ← WebAppEngine
│  │   ├─ ts_tool.rs          ← TsTool
│  │   └─ ... (lock, utils)
│  ├─ commands/
│  │   ├─ run_ts.rs           ← Command mới
│  │   └─ ... (install, update...)
│  └─ main.rs                 ← Thêm subcommand
├─ docs/
│  └─ tasks/
│      └─ webapp-core-req.md  ← File yêu cầu này
└─ .github/
   └─ workflows/
       └─ webapp-core-ci.yml  ← CI mẫu
```

## 6. Tiêu chí chấp nhận (Acceptance Criteria)

- [ ] `MegaGate install` tự động dùng Bun/PNPM/npm trong dự án JS/TS.
- [ ] `MegaGate run-ts build` bundle thành công dự án TS.
- [ ] `MegaGate run-ts dev` khởi chạy dev-server với HMR.
- [ ] `MegaGate run-ts lint` chạy ESLint.
- [ ] `MegaGate run-ts test` chạy test (Jest/Vitest/Bun test).
- [ ] Unit test cover ≥ 80% logic core.
- [ ] CI workflow chạy pass trên 3 nền tảng.
- [ ] Tài liệu `CORE.md` mô tả đầy đủ cách dùng và mở rộng.

## 7. Rủi ro & Giảm thiểu

| Rủi ro | Giảm thiểu |
|--------|-----------|
| Bun/PNPM không cài đặt trên máy user | Fallback tự động sang npm, thông báo rõ ràng. |
| Xung đột phiên bản TS | Dùng `tsc --noEmit` để check type trước khi bundle. |
| Dev-server không tương thích | Ưu tiên Vite (ổn định), fallback Bun dev. |
| Lỗi mock trong test | Sử dụng `assert_cmd` hoặc `mockall` để mock Command. |

## 8. Người phụ trách & Liên hệ

- **Developer**: [Tên người thực hiện]
- **Reviewer**: [Tên người review]
- **Ngày bắt đầu**: [Điền ngày]
- **Ngày dự kiến hoàn thành**: [Điền ngày + 8 ngày]

---

*Lưu ý: Cập nhật trạng thái từng giai đoạn vào bảng tiến độ khi hoàn thành.*