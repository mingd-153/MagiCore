# AGENTS.md — MegaGate

- Đọc `RULE.md` trước MỌI task — bắt buộc (workflow 5 bước, naming, song ngữ).
- Nguồn chân lý: `sys-mg/11-folder-structure.md` (cây folder) + `sys-mg/00-index.md` (quyết định).
- Báo cáo tiếng Việt; code/design không tự ý vượt phase đã duyệt.
- Task rà soát/phân tích/maintain codebase (hoặc user kéo folder vào) → đọc `CODEBASE_REVIEW.md` và tuân theo flow (chờ user kéo folder → index GitNexus → review → báo cáo).

## Quy trình PHÁT TRIỂN BẮT BUỘC (mandatory — DEFINE → SHIP)

Mọi task phát triển (feature / fix / refactor / core mới) bắt buộc đi qua pipeline 6 bước, **chạy ĐỦ 2 vòng lặp**, rồi **dừng đợi phê duyệt**.

```
DEFINE → PLAN → BUILD → VERIFY → REVIEW → SHIP
                  └──────── ← ─────┘  (vòng 2: BUILD→VERIFY→REVIEW lặp lại)
```

| Bước | Yêu cầu tối thiểu | Skill tương ứng (đã cài) |
|---|---|---|
| 1. DEFINE | Spec/PRD rõ trước code: mục tiêu, phạm vi, ràng buộc, reference RULE + sys-mg | `spec-driven-development`, `interview-me` (nếu yêu cầu mập mờ) |
| 2. PLAN | Task nhỏ, atomic, criterion chấp nhận, thứ tự dependency, todo list | `planning-and-task-breakdown` |
| 3. BUILD | Từng slice nhỏ, test-driven, an toàn (fail-closed + escape hatch), bám RULE | `incremental-implementation`, `test-driven-development`, `source-driven-development`, `security-and-hardening` (khi chạm input/auth/storage) |
| 4. VERIFY | Chạy test riêng + chung (`cargo test --workspace`), sửa fail, bằng chứng pass | `debugging-and-error-recovery` (khi fail), `browser-testing-with-devtools` (web runtime) |
| 5. REVIEW | Self-review đầy đủ diff, tìm lỗi + bypass, ghi finding | `code-review-and-quality`, `doubt-driven-development` (rủi ro cao), `code-simplification`, `performance-optimization` |
| 6. SHIP | Chỉ sau VÒNG 2 sạch. Báo cáo tiếng Việt (RULE §6) + docs/ + checklist. **DỪNG đợi phê duyệt** — không commit/push khi chưa user duyệt | `shipping-and-launch`, `git-workflow-and-versioning` |

**Quy tắc 2 vòng (loop twice):**
- Vòng 1: chạy đủ DEFINE→REVIEW. Findings từ REVIEW vòng 1 nạp vào BUILD vòng 2 (không bỏ sót).
- Vòng 2: BUILD lại (sửa findings) → VERIFY lại (test pass) → REVIEW lại (diff phải sạch, không còn finding blocking).
- SHIP chỉ được bước khi REVIEW vòng 2 thông. SHIP xong → **dừng đợi phê duyệt user** trước commit/push/PR.
- Task nhỏ (<3 thao tác, không logic mới): vẫn bám quy trình, có thể gộp gọn nhưng không bỏ VERIFY/REVIEW; vẫn 2 vòng nếu chạm logic.

Dùng skill tool (`using-agent-skills`) để chọn + thực thi skill đúng bước; skill bắt buộc KHÔNG được bỏ nếu áp dụng.
