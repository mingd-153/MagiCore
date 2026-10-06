# Task đang theo dõi

## Chức năng

Thư mục lưu kế hoạch cùng checklist/bằng chứng cho lát công việc parse lockfile có version nghiêm ngặt, gồm từ chối field lạ và coverage digest/chữ ký v4.

## Nội dung

- `plan.md` — task goal, scope, acceptance criteria, and verification commands.
- `todo.md` — checklist, test evidence, and recorded limitations.

## Trạng thái và cách dùng

`plan.md` nêu phạm vi và tiêu chí nhận; `todo.md` ghi việc đã làm và bằng chứng kiểm chứng, gồm cả gate toàn cục lỗi hoặc chưa hoàn tất. Coi đây là hồ sơ task theo thời điểm, không phải tuyên bố toàn workspace hay release đã xanh. Chạy lại kiểm tra liên quan trên source hiện tại trước khi dựa vào kết quả.
