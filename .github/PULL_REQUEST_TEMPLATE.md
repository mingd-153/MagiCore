## Mô tả

<!-- Sửa gì / tại sao — tối đa vài dòng -->

## Checklist

- [ ] Design/spec file nào đụng: `docs/specs/...` (nếu có)
- [ ] Test thêm: `<tên test>` (hoặc "không — lý do")
- [ ] `cargo test --workspace` pass
- [ ] `cargo fmt --all --check` pass
- [ ] `cargo clippy --all-targets -- -D warnings` pass
- [ ] `bash scripts/check-module-hygiene.sh` pass (L1 cấm import chéo)
- [ ] Entry append vào `docs/specs/megaGateChangeLog.md` (BẮT BUỘC — user 2026-08-15)
- [ ] RULE thay đổi? → cập nhật AGENTS.md/RULE.md + changeLog

## Nuances

<!-- Bypass mặc định? Log secret? (18 §3.3) — nêu rõ nếu có -->