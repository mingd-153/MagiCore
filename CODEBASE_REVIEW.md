# CODEBASE_REVIEW.md — Quy trình rà soát codebase / Codebase Review Procedure

> Cập nhật cấu trúc: 2026-10-06.
> This file defines the review procedure; it is not a claim that every feature is complete or passing.
> Bản đồ vị trí hiện hành nằm trong [codebaseRoadmap.md](codebaseRoadmap.md).

## 1. Khi nào áp dụng / When to apply

- Khi user yêu cầu quét, rà soát, phân tích, maintain codebase hoặc một phạm vi thư mục.
- Trước release/RC: kiểm tra evidence gắn với đúng commit đang đánh giá.
- Sau diff lớn hoặc trước khi kết luận về merge/release readiness.

## 2. Cây thư mục và nguồn kiểm tra / Repository layout and evidence sources

Dùng [codebaseRoadmap.md](codebaseRoadmap.md) làm chỉ mục đường dẫn; xác nhận lại trên checkout hiện tại bằng find, rg --files, manifests và workflow files. Không xem số lượng hoặc tên thư mục là bằng chứng capability.

| Vị trí | Vai trò định vị |
|---|---|
| <code>core/crates/</code> | Các crate dùng chung; Cargo manifest là nguồn xác nhận membership. |
| <code>adapters/</code> | Adapter theo nhóm ecosystem/core: ai, app, cicd, cloud, game, hardware, iot, lib, web. |
| <code>cli/src/</code>, <code>cli/tests/</code>, <code>cli/embedded/</code> | CLI, test tích hợp của CLI và tài nguyên nhúng. |
| <code>tools/</code> | Công cụ workspace và công cụ bị loại khỏi workspace; đối chiếu Cargo.toml. |
| <code>templates/</code> | Template nguồn; tại snapshot 2026-10-06 chỉ thấy <code>templates/web/</code>. |
| <code>tests/e2e/</code>, <code>tests/fixtures/</code> | E2E cấp workspace và fixtures. |
| <code>scripts/</code>, <code>ci/</code>, <code>.github/workflows/</code> | Script kiểm tra, evidence CI và workflow. |
| <code>benchmark/</code>, <code>assets/</code>, <code>data/</code>, <code>deploy/</code>, <code>packaging/</code> | Khu vực benchmark, asset, runtime data, cấu hình deploy và phân phối. |
| <code>tasks/</code> | Tài liệu task hiện có; kiểm tra file thật trước khi dựa vào task status. |
| <code>docs/</code> | Tài liệu nội bộ local-only theo .gitignore, trừ khi user cho phép chính xác file khác. |

Nguồn xác định hành vi: source, Cargo/workspace manifests, tests, scripts/workflows và output đã chạy. Markdown cũ chỉ là bối cảnh, không phải bằng chứng triển khai.

## 3. Quy trình rà soát / Review sequence

1. **Chốt scope**: ghi rõ mục tiêu, phần được kiểm tra, điều được user loại trừ.
2. **Chụp baseline**: branch, HEAD, git status --short, diff và test/CI run liên quan. Không gán kết quả của SHA khác cho HEAD hiện tại.
3. **Dò cấu trúc thật**: xác nhận đường dẫn trong roadmap với checkout và manifest. Nếu thiếu hoặc đổi tên, cập nhật map trong phạm vi được giao.
4. **Đọc triển khai**: truy từ CLI/API entry point tới adapter/crate, manifest, test, scripts và workflow liên quan. Ưu tiên evidence đọc trực tiếp; dùng GitNexus nếu active instructions yêu cầu và user không loại trừ công cụ đó. Nếu bị loại trừ, truy call-site thủ công và ghi rõ giới hạn.
5. **Xác minh từng claim**: tách capability theo thao tác; folder tồn tại hoặc workflow xanh không chứng minh toàn bộ lifecycle.
6. **Báo cáo tiếng Việt**: nêu kết quả, evidence/path hoặc lệnh và exit status, phạm vi chưa xác minh, mức độ hỗ trợ thực tế.

## 4. Quy tắc bằng chứng / Evidence rules

- Source claim phải chỉ ra file và symbol/dòng; command claim phải giữ nguyên command, exit status và môi trường liên quan.
- CI claim phải gắn run với đúng commit; đọc job kết thúc và log khi có failure.
- FAIL là fail; skipped, ignored, chưa chạy, thiếu tool hoặc timeout không được ghi thành pass.
- Không suy ra capability từ tên workflow (ví dụ “full lifecycle”), tên adapter hoặc folder.
- Ghi rõ khi evidence do user cung cấp và chưa được agent tái hiện độc lập.
- Không kết luận merge/release readiness từ test subset hoặc kết quả ở commit khác.

### Trạng thái test được user đính chính — 2026-10-06

User xác nhận lượt test được nhắc tới đã **FAIL**. Chỉ ghi nhận trạng thái đó; không tự tạo run ID, SHA, job name hoặc nguyên nhân. Trước khi gắn lỗi vào một commit cụ thể, lấy run/log thực tế của commit đó.

## 5. Tài liệu và ghi nhận sửa đổi / Documentation handling

- Append thay đổi review vào <code>docs/specs/magiCoreChangeLog.md</code> theo <code>AGENTS.md</code>; changelog này ở <code>docs/</code> nên local-only và không được stage/push nếu chưa được user cho phép đúng file.
- Root <code>CODEBASE_REVIEW.md</code> và <code>codebaseRoadmap.md</code> là hướng dẫn dùng chung được user cho phép đưa vào Git. Điều này không mở quyền publish các file khác trong <code>docs/</code>.
- Không xóa hoặc coi tài liệu lịch sử là source. Sửa trạng thái trong tài liệu theo evidence mới, giữ rõ thời điểm và phạm vi.
