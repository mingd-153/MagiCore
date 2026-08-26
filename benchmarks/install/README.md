# Install Benchmark — mgc vs npm vs pnpm

Câu hỏi: `mgc install` nhanh/chậm cỡ nào so với npm & pnpm, cold lẫn warm, tốn bao nhiêu disk?

## Cách chạy

```bash
cargo build --release -p mgc --bin mgc
scripts/benchmark-install.sh            # (runs_cold=2, runs_warm=3 mặc định)
```

## Hai chế độ

| Mode | Lệnh | Ý nghĩa |
|------|------|---------|
| `REGISTRY_MODE=local` ✅ chuẩn public | mọi tool trỏ vào `mgc-registry --upstream` local | So **tốc độ tool thuần** — không nhiễu mạng |
| `REGISTRY_MODE=real` | bắn thẳng registry.npmjs.org | Thực tế người dùng cuối, nhưng stddev cao |

## TODO (chưa public)
- Multi-project shared-store scenario: phát hiện mgc đang COPY thay vì hardlink từ CAS
  trên macOS (`node_modules` ×5 ≈ 5×dung lượng) → cần điều tra/fix link-mode trước khi đo,
  nếu không số sẽ tự vạ miệng.
- Cột bun.

## Phương pháp

- Fixture: 10 direct deps version pin cứng (react/express/typescript/zod…) — mọi tool giải cùng một cây
- **Cold**: trước mỗi lần đo xoá node_modules + toàn bộ cache/store của tool (mgc qua `HOME` cô lập; npm/pnpm qua `npm_config_cache`/`npm_config_store_dir`)
- **Warm**: 1 lần nạp cache ngầm, giữa các lần chỉ xoá node_modules
- Timing: hyperfine; disk: `du -sk node_modules`
- Binary mgc: build **release**, ghi kèm commit hash

## Kết quả

Xem `results-*.md` trong thư mục này (mỗi lần chạy một file, có ngày + thông số máy).
