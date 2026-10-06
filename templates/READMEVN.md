# Workspace template của MagiCore và cây đầu vào phục vụ phát triển

## Chức năng

Thư mục này là cây ghi đè phục vụ phát triển/kiểm thử và là đích của công cụ sinh template. Đây không phải catalog template production. CLI đọc nội dung từ `MAGICORE_TEMPLATE_DIR` khi được đặt tường minh; nếu không thì dùng template cache của người dùng (`~/.mgc/templates`). `TemplateRoot::resolve` không fallback sang cây workspace trong repository.

Nội dung scaffold có sẵn ban đầu gồm 7 Markdown fragment được Git theo dõi dưới `sources/`; các cặp README cấp thư mục chỉ là tài liệu mô tả. Cây không có hợp đồng `template.toml` và không có payload source/config dự án ngoài Markdown, nên hiện tại thư mục tự nó có **0 template layer có thể publish**. Thư mục rỗng và README fragment không chứng minh framework được hỗ trợ.

Cây này được E2E harness (`tests/e2e/src/lib.rs`) tham chiếu qua `MAGICORE_TEMPLATE_DIR`, được generator như `scripts/gen-backend.sh`, `scripts/generate-feature-templates.sh` và script migration dùng để tạo cấu trúc. Workflow CI riêng lấy cây template từ một revision Git cũ vào thư mục tạm rồi chạy `mgc template publish-all` trên đầu vào đó. Chỉ xóa thư mục nếu đồng thời chuyển hoặc gỡ có chủ đích các tham chiếu phát triển/kiểm thử/sinh template này. Lưu ý `scripts/migrate-to-new-structure.sh` ghi đè `templates/README.md` bằng nội dung chung; nếu chạy lại script, cần đồng bộ generator trước để giữ hướng dẫn chi tiết.

## Nội dung

- [`web/READMEVN.md`](web/READMEVN.md) — cây scaffold web và giới hạn hiện tại.
- [`web/shared/READMEVN.md`](web/shared/READMEVN.md) — các mảnh web dùng chung.
- [`web/shared/partials/READMEVN.md`](web/shared/partials/READMEVN.md) — vai trò các partial layer.
- README từng partial: [`base`](web/shared/partials/base/READMEVN.md), [`frontend Rust-ready`](web/shared/partials/frontend-rust-ready/READMEVN.md), [`monorepo Rust-ready`](web/shared/partials/monorepo-frontend-rust-ready/READMEVN.md), [`monorepo packages`](web/shared/partials/monorepo-packages/READMEVN.md) và [`monorepo`](web/shared/partials/monorepo/READMEVN.md).
- Bảy file hiện có dưới `sources/` là README hướng tới scaffold được sinh ra. Giữ chúng làm tài liệu payload template; chúng không phải hướng dẫn folder cho agent.
- `mgc template publish-all` chỉ nhận layer có cả `template.toml` và `sources/`. Checkout hiện tại không có hợp đồng nào như vậy.

## Trạng thái

README này mô tả vai trò thư mục và giới hạn có bằng chứng từ cây file hiện tại. Hãy kiểm tra lại source, manifest và test trước khi dựa vào thay đổi sau này.
