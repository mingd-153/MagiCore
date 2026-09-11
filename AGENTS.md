# AGENTS.md — MagiCore

- Đọc `RULE.md` trước MỌI task — bắt buộc (workflow 5 bước, naming, song ngữ).
- Nguồn chân lý: `sys-mgc/11-folder-structure.md` (cây folder) + `sys-mgc/00-index.md` (quyết định).
- Báo cáo tiếng Việt; code/design không tự ý vượt phase đã duyệt.
- **BÁO CÁO SAU MỖI LẦN SỬA (BẮT BUỘC, user 2026-08-15):** sau mỗi lần sửa xong (code/design/docs/RULE) → APPEND 1 entry vào `docs/specs/magiCoreChangeLog.md` — ghi: sửa gì, động file nào, RULE đổi gì (nếu có), thời gian ISO. KHÔNG sửa đè entry cũ. RULE thay đổi → cập nhật AGENTS.md/RULE.md + ghi vào changeLog. **Changelog/docs nội bộ là local-only mặc định: không `git add`, không `git add -f`, không commit/push bất kỳ `.md` trong `docs/` nếu user chưa cho phép đúng file đó.**
- **CORE PARITY (user 2026-08-25):** không dồn năng lực vào mỗi `web`; khi thêm CLI/scaffold/test/quality gate phải cân bằng tối thiểu 4 core chính `web`, `ai`, `app`, `lib` hoặc ghi rõ lý do scope cục bộ.
- Task rà soát/phân tích/maintain codebase (hoặc user kéo folder vào) → đọc `CODEBASE_REVIEW.md` và tuân theo flow (chờ user kéo folder → index GitNexus → review → báo cáo).

## Quy trình PHÁT TRIỂN BẮT BUỘC (mandatory — DEFINE → SHIP)

Mọi task phát triển (feature / fix / refactor / core mới) bắt buộc đi qua pipeline 6 bước, **chạy ĐỦ 2 vòng lặp**, rồi **dừng đợi phê duyệt**.

```
DEFINE → PLAN → BUILD → VERIFY → REVIEW → SHIP
                  └──────── ← ─────┘  (vòng 2: BUILD→VERIFY→REVIEW lặp lại)
```

| Bước | Yêu cầu tối thiểu | Skill tương ứng (đã cài) |
|---|---|---|
| 1. DEFINE | Spec/PRD rõ trước code: mục tiêu, phạm vi, ràng buộc, reference RULE + sys-mgc | `spec-driven-development`, `interview-me` (nếu yêu cầu mập mờ) |
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

This project is indexed by GitNexus as **MagiCore** (10929 symbols, 26830 relationships, 836 execution flows).

> Index stale? Run `node .gitnexus/run.cjs analyze --index-only` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? Bootstrap with `npx`, `bunx`, or `pnpm dlx` — e.g. `bunx gitnexus@latest analyze` (npm 11 npx crash; #1939).

## Always Do

- **MUST run impact before editing.** Use `impact({target: "symbolName", direction: "upstream"})` or `node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .`; report callers, processes, and risk. Never substitute grep for graph analysis.
- **MUST analyze graph changes before committing.** Use `detect_changes({scope: "all"})` (MCP) or `node .gitnexus/run.cjs detect-changes --scope all --repo .` (CLI fallback). `partial: true` or `truncated: true` is not a clean check — a zero means unseen, not unaffected; re-run it. For regression review: `detect_changes({scope: "compare", base_ref: "main"})` or `node .gitnexus/run.cjs detect-changes --scope compare --base-ref "main" --repo .`.
- MUST warn on HIGH/CRITICAL `risk` pre-edit; never use `riskSharedAxes` to waive a HIGH/CRITICAL `risk` warning. Compare File/symbol: MCP File omits axes; Graph-RAG expands File.
- **MUST treat `risk: UNKNOWN` as unresolved, not as low.** An empty caller set is not evidence the symbol is unused — it can also mean the callers are not resolvable by the index (plain-object property access, dynamic dispatch, cross-language calls). `impact` pairs `UNKNOWN` with a `riskNote` saying so. Confirm with a text search before treating the symbol as safe to change or delete; do not proceed on the strength of a zero.
- **MUST use `query({search_query: "concept"})` for concepts/flows, `context({name: "symbolName"})` for a named symbol, or `impact` for blast radius, on read-only callers, dependencies, imports, or execution flow.** Graph first; text search only for empty/`UNKNOWN`/literals.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method before MCP/CLI impact analysis.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis, and never read `UNKNOWN` as an all-clear — it means the walk could not answer, which is the one verdict that requires confirming by other means.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit before MCP/CLI graph change analysis.

## Resources

| Resource | Use for |
| --- | --- |
| `gitnexus://repo/MagiCore/context` | Codebase overview, check index freshness |
| `gitnexus://repo/MagiCore/clusters` | All functional areas |
| `gitnexus://repo/MagiCore/processes` | All execution flows |
| `gitnexus://repo/MagiCore/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
| --- | --- |
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
