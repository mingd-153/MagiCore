# AGENTS.md — MegaGate

- Đọc `RULE.md` trước MỌI task — bắt buộc (workflow 5 bước, naming, song ngữ).
- Nguồn chân lý: `sys-mg/11-folder-structure.md` (cây folder) + `sys-mg/00-index.md` (quyết định).
- Báo cáo tiếng Việt; code/design không tự ý vượt phase đã duyệt.
- **BÁO CÁO SAU MỖI LẦN SỬA (BẮT BUỘC, user 2026-08-15):** sau mỗi lần sửa xong (code/design/docs/RULE) → APPEND 1 entry vào `docs/specs/megaGateChangeLog.md` — ghi: sửa gì, động file nào, RULE đổi gì (nếu có), thời gian ISO. KHÔNG sửa đè entry cũ. RULE thay đổi → cập nhật AGENTS.md/RULE.md + ghi vào changeLog.
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

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **MegaGate** (6088 symbols, 13400 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/MegaGate/context` | Codebase overview, check index freshness |
| `gitnexus://repo/MegaGate/clusters` | All functional areas |
| `gitnexus://repo/MegaGate/processes` | All execution flows |
| `gitnexus://repo/MegaGate/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
