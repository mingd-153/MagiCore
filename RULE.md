# RULE.md — Quy tắc bắt buộc cho AGENT (Agent Compliance Rules)

> Version: 1.0 | Ngày: 2026-08-04
> Đây là quy tắc **bắt buộc tuyệt đối**. Mọi task trong repo này phải tuân thủ.
> This is an absolutely mandatory rule. Every task in this repository must comply.
> Vi phạm = làm lại. Violation = redo.

---

## 1. FOLDER-STRUCTURE — BẮT BUỘC TUÂN THEO (Mandatory Structure)

Agent **bắt buộc** tuân theo cây folder tại: `sys-mg/11-folder-structure.md` (source of truth — nguồn chân lý duy nhất).

- Không tạo file/folder ngoài cây được phép. No files/folders outside the allowed tree.
- Cây có thể thay đổi **chỉ khi** user duyệt bản cập nhật 11-folder-structure.md trước. Tree changes only after user approves.
- Nghi ngờ → dừng hỏi. Uncertain → stop and ask. KHÔNG tự quyết định. NEVER guess.

---

## 2. CẤU TRÚC FOLDER CHUẨN — src/ + docs/ + test/ (Standard Folder Layout)

Khi bắt đầu làm việc trong bất kỳ folder nào (core crate, adapter, cli, template...), **bắt buộc** cấu trúc:

```
tên-folder/
├── src/                # code nguồn
├── docs/               # báo cáo tiến độ + checklist (viết thường — lowercase)
│   ├── ProgressReport.md    # Báo cáo tiến độ — viết sau MỖI lần chạy workflow (§4)
│   └── Checklist.md         # Checklist công việc của folder — tick [x] khi xong (§3)
├── test/               # test riêng của folder (test module, KHÔNG đặt trong src)
├── README.md           # mô tả folder
└── (file config khác)
```

- `docs/` là nơi duy nhất ghi báo cáo tiến độ + checklist của folder đó.
- docs/ is the single place for progress reports + checklists.
- `test/` là nơi duy nhất đặt test của folder — **CẤM `#[cfg(test)]` inline trong `src/`** (xem §5).
- Ví dụ: `core/crates/mg-exec/docs/Checklist.md`, `adapters/game/test/`, `cli/docs/ProgressReport.md`.
- Tên folder phải là **lowercase** (`docs/` không phải `DOCS/`).

---

## 3. CHECKLIST — QUY TRÌNH BẮT BUỘC (Mandatory Workflow)

Mỗi task (viết code / sửa bug / viết design / thêm tính năng) phải đi qua **5 bước đúng thứ tự**:

| Bước | Hành động | Bắt buộc |
|---|---|---|
| 1 | **VIẾT** (Write) — viết code/design theo đúng TASK + REQUIREMENT + DESIGN | ✅ |
| 2 | **LOOP AUDIT DEEPLY** (Audit #1) — đọc lại toàn bộ thay đổi, tự tìm lỗi (logic, edge case, bảo mật, tuân thủ RULE) → sửa → lặp đến sạch. **Bắt buộc trả lời 2 câu hỏi phản biện (§11)** | ✅ |
| 3 | **TEST** — chạy TEST RIÊNG (§5) + TEST CHUNG (§5), tất cả pass | ✅ |
| 4 | **LOOP AUDIT DEEPLY** (Audit #2) — audit lại lần 2 toàn bộ diff (có gì bị bỏ sót sau test?) → sửa nếu cần. **Trả lời lại 2 câu hỏi phản biện (§11)** | ✅ |
| 5 | **BÁO CÁO** — báo cáo bằng tiếng Việt (§6) | ✅ |

Trong `Checklist.md` của folder, mỗi task ghi 5 dòng tick:

```markdown
## Task: <tên task>
- [x] 1. VIẾT — code/design hoàn tất
- [x] 2. LOOP AUDIT DEEPLY (#1) — sạch lỗi tự tìm
- [x] 3. TEST — test riêng + test chung PASS
- [x] 4. LOOP AUDIT DEEPLY (#2) — diff sạch
- [x] 5. BÁO CÁO — đã gửi tiếng Việt
```

- **Chỉ tick `[x]` khi bước đó THỰC SỰ xong** (test pass thật, audit thật). Tick giả = vi phạm.
- Only tick [x] when the step is actually done.
- Task chưa xong → để `[ ]` + ghi lý do trong ProgressReport.md.

---

## 4. TASK + REQUIREMENT + DESIGN — ĐỊNH NGHĨA RÕ 3 THỨ

Mọi việc làm đều thuộc 1 trong 3 loại — ghi rõ ở đầu ProgressReport.md:

| Loại | Định nghĩa | Ví dụ |
|---|---|---|
| **TASK** | Việc được giao (từ user / plan) | "Implement mg-exec allowlist" |
| **REQUIREMENT** | Yêu cầu ràng buộc phải thỏa (từ design MD trong sys-mg/) | "Allowlist bất biến 25 tool, cấm npm" |
| **DESIGN** | Thiết kế quyết định trước khi viết (trong sys-mg/ hoặc docs/ của folder) | "mg-exec module map, audit sanitizer" |

- **Requirement/Design thiếu → KHÔNG được viết code** — hỏi user trước. No requirement/design = no code.
- Mọi thay đổi code phải trỏ tới requirement trong sys-mg/ (ghi số file + section). Every code change must reference its requirement.

---

## 5. TEST RIÊNG + TEST CHUNG (Per-Unit + Whole-System Tests)

| Loại | Phạm vi | Bắt buộc khi |
|---|---|---|
| **TEST RIÊNG** | Test của folder/crate đó — **bắt buộc đặt tại `<folder>/test/`** (integration test, không `#[cfg(test)]` trong `src/`) | Mỗi task trong folder |
| **TEST CHUNG** | Toàn workspace + E2E (`cargo test --workspace`, `tests/e2e/`) | Mỗi task, trước khi báo cáo |

Quy tắc:
- Test RIÊNG trước (nhanh, cụ thể), rồi TEST CHUNG (đảm bảo không phá phần khác).
- Non-trivial logic: BẮT BUỘC có ≥1 test (ponytail rule). Chạy test trong bước 3 — không báo cáo "code xong" khi test chưa pass.
- **CẤM đặt test trong `src/`** (không dùng `#[cfg(test)] mod tests` inline). Mọi test phải nằm ở `<folder>/test/`.
  - Rust: test files đặt trong `test/`, khai báo target trong `Cargo.toml` (`[[test]] name = "..." path = "test/...rs"`) nếu crate dùng thư mục `test/` thay vì `tests/`.
  - Với crate có sẵn `tests/` (chuẩn Cargo): giữ nguyên — `tests/` cũng là vị trí hợp lệ cho integration test. Điểm mấu chốt: test KHÔNG nằm chung với code production trong `src/`.
- Tóm tắt: production code trong `src/`; test trong `test/` (hoặc `tests/` chuẩn Cargo) — tách bạch.

---

## 6. BÁO CÁO — TIẾNG VIỆT (Report in Vietnamese)

- Mọi báo cáo gửi user: **tiếng Việt**. All reports to user: Vietnamese.
- Nội dung tối thiểu: (1) đã làm gì, (2) kết quả test riêng/chung, (3) tuân thủ RULE ra sao, (4) còn thiếu/đang chờ gì.
- Cập nhật `ProgressReport.md` của folder song song với báo cáo.

---

## 7. COMMENT — SONG NGỮ + CONSOLE TIẾNG ANH (Bilingual Comments + English Console)

Mọi comment trong code: **song ngữ Anh – Việt** (2 dòng hoặc cùng dòng):

```rust
// Allowlist check — kiểm tra allowlist trước khi exec
// Redacts sensitive args — che đối số nhạy cảm
```

**Console output (error messages, logs, CLI output, panic messages): TIẾNG ANH** — để dễ debug, grep, internationalization.

```rust
// ✅ ĐÚNG: console English, comment song ngữ
bail!("manifest is not a JSON object");  // manifest không phải JSON object

// ❌ SAI: console tiếng Việt
bail!("manifest không phải JSON object");
```

Quy tắc:
- EN mô tả kỹ thuật; VI mô tả ý nghĩa/ngữ cảnh (không dịch nguyên văn lặp lại).
- File mới: header 2 dòng song ngữ: mô tả file (EN) + mục đích (VI).
- Chỉ comment ý nghĩa thật — không comment tầm thường (`// increment i`).
- **Console/log/panic/error messages: ENGLISH ONLY** — messages hiển thị ra terminal, log file, CLI output đều bằng tiếng Anh.

---

## 8. ĐẶT TÊN — RÕ RÀNG, TƯỜNG MINH, ĐÚNG CASE (Naming Convention)

| Loại | Quy tắc | Ví dụ |
|---|---|---|
| Folder | lowercase, rõ nghĩa | `core/crates/mg-exec`, `adapters/game` |
| Rust module file | **snake_case** (chuẩn Rust bắt buộc) | `mg_exec/allowlist.rs` |
| Rust type (struct/trait) | **PascalCase** | `ExecReport`, `PackageAdapter` |
| Rust const/static | **UPPER_SNAKE_CASE** | `FORBIDDEN_TOOLS` |
| CLI command | verb-object, lowercase, không gạch | `mg publish`, `mg store gc` |
| CLI flag | kebab-case | `--dry-run`, `--no-git-checks` |
| Config key (mg.toml) | snake_case | `store.max_gb`, `scripts.mode` |
| File báo cáo/checklist/docs | **camelCase** | `ProgressReport.md`, `Checklist.md`, `networkVerify.md` |
| File phải viết hoa | ĐÚNG như tên bắt buộc | `LICENSE`, `README.md`, `RULE.md`, `CHANGELOG.md`, `Cargo.toml` |
| Folder docs/tests | **lowercase** (viết thường) | `docs/`, `test/`, `tests/` — KHÔNG `DOCS/` |

- Tên **phải mô tả trách nhiệm** (không `utils.rs`, `misc.rs`, `helper.rs`). Name = responsibility.
- Cấm từ "component" làm tên folder/file (nhầm web component). "component" is forbidden as a name.

---

## 9. ĐIỀU CẤM TUYỆT ĐỐI (Absolute Forbidden)

1. Tự ý tạo folder/file ngoài cây đã duyệt. Creating files outside the approved tree.
2. Tự ý sửa design MD trong sys-mg/ khi chưa được duyệt. Editing design MDs without approval.
3. Đoán mò khi không chắc — phải dừng hỏi user. Guessing when unsure — ask.
4. Commit/push khi chưa được yêu cầu. Committing without being asked.
5. Viết code khi thiếu REQUIREMENT/DESIGN. Writing code without requirements.
6. Báo cáo "xong" khi test chưa pass hoặc checklist chưa tick. Reporting done without green tests.
7. Log/in in secret (token/password/OTP) dưới mọi hình thức. Logging secrets in any form.
8. **Hardcode giá trị có thể config** (đường dẫn, URL, kích thước giới hạn, timeout, registry, cổng...) — mọi giá trị đổi được theo môi trường phải qua config (mg.toml / env / const tập trung). Hardcoding configurable values is forbidden.
9. **Test trong `src/`** (không dùng `#[cfg(test)]` inline) — test phải ở `test/` (hoặc `tests/` chuẩn Cargo). Tests in src/ are forbidden.

---

## 10. THAM CHIẾU (References)

| File | Vai trò |
|---|---|
| `sys-mg/11-folder-structure.md` | Cây folder tổng thể — nguồn chân lý |
| `sys-mg/00-index.md` | 24 quyết định Q1–Q24 + exec policy + phase |
| `sys-mg/14-module-map.md` | Module map từng crate |
| `sys-mg/21-repo-ops.md` | Quality gates, CI, PR convention |
| `CONTRIBUTING.md` | Hướng dẫn contribute (đã có) |
| `LICENSE` | MIT (đã có) |

## 11. 2 CÂU HỎI PHẢN BIỆN BẮT BUỘC (bước 2 và 4 — bài học phản biện v2)

Trong MỌI lần LOOP AUDIT DEEPLY, phải tự trả lời **2 câu hỏi** (trả lời trong ProgressReport.md):

1. **Còn cửa bypass nào?** — Kẻ tấn công/người dùng vượt được cơ chế này không? (vd: passthrough thuần không verify lockfile; trust theo tên thuần bị typosquat; lockfile chuẩn không có chữ ký; hardlink khác phân vùng lỗi im lặng)
2. **Chặn cứng có phá luồng dùng thật?** — Fail-closed đến mức vô dụng không? (vd: offline cấm stale metadata = không cài lại được trên máy bay; local fail-fast = sửa lỗi từng con một)

**Nguyên tắc Fail-closed + Escape hatch:** mọi cơ chế chặn đều cần:
- Mặc định CHẶN (fail-closed) ✅
- Nhưng có escape hatch hợp lệ + **cảnh báo rõ ràng** (vd: stale metadata offline → warning vàng; non-scoped private → allowlist tường minh; sandbox AI/IoT → opt-in) ✅
- Escape hatch không bao giờ im lặng — luôn ghi audit hoặc cảnh báo lần dùng đầu

## 12. CẤM HARDCODE (No Hardcoding)

Mọi giá trị có thể thay đổi theo môi trường/config **bắt buộc** đưa ra khỏi code:

| Giá trị | Nơi đặt |
|---|---|
| Đường dẫn (store, cache, log, lock...) | config (mg.toml / env) — `mg-config/paths.rs`, `mg-platform/paths.rs` |
| URL registry / API endpoint | config (mg.toml `[registry]`, .npmrc) — KHÔNG nhúng `https://registry.npmjs.org` trong code logic |
| Giới hạn kích thước / timeout / retry | config hoặc const tập trung đầu file có tên mô tả (`MAX_*`, `DEFAULT_*`) |
| Cổng mạng, host | config / env (`MG_*_PORT`) |
| Danh sách (allowlist, forbidden) | config hoặc const tập trung — không rải rác trong logic |

Quy tắc:
- Giá trị default hợp lý: đặt const tập trung đầu module (`pub const DEFAULT_REGISTRY: &str = ...`) — vẫn ghi đè được bằng config. Defaults are overridable, not hardcoded inline.
- Không nhúng literal trong lòng logic (`if size > 100_000_000` → `if size > MAX_TARBALL_SIZE`).
- Violation = redo (§1).

---

## 13. PORT CONVENTION — MỌI PORT PHẢI CHỨA ĐỦ 4 CHỮ SỐ 4·3·1·5 (Port Rule)

Mọi cổng mạng dùng trong dự án **bắt buộc chứa đủ 4 chữ số: 4, 3, 1, 5** (thứ tự bất kỳ, mỗi số đúng 1 lần — port phải là hoán vị của 4 chữ số này). Every port must contain all of digits 4, 3, 1, 5.

- Áp dụng cho: port mặc định trong code (`DEFAULT_*_PORT`), config mẫu (mg.toml, .env.example), docker-compose, docs, script test, CLI flag.
- Chỉ còn **24 số hợp lệ** (hoán vị của 4315): `4315 4351 4135 4153 4513 4531 3415 3451 3145 3154 3541 3514 1345 1354 1435 1453 1534 1543 5134 5143 5314 5341 5413 5431`.
- Port mặc định chính thức của registry: **4315** (mg-registry). Các service khác chọn hoán vị khác nhau trong 24 số trên — không trùng lặp.
- CẤM port không đủ 4 chữ số đó (vd: `8080`, `3000`, `18091`, `5432`). Forbidden: any port without all four digits.
- Violation = redo (§1).

---
