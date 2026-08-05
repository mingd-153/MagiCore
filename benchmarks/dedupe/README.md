# Dedupe Benchmark (02 §4)

Project chuẩn đo hiệu quả dedupe: react@18.0.0 (pin) + 6 libs peer react (react-router-dom, @tanstack/react-query, zustand, @mui/material, framer-motion, react-dropzone).

## Câu hỏi

1. `--prefer-dedupe` giảm bao nhiêu instance so với mặc định (PreferLatest)?
2. Install time delta ≤ 5%?
3. Disk saving vstore?

## Cách chạy

```bash
MEGAGATE_WEB_STRICT_LAYOUT=1 ./bench.sh
```

Script:
1. Seed: project A install react@18.0.0 pin (lock có instance 18.0.0).
2. Baseline: project A thêm libs, `mg install` (PreferLatest) → đếm instance, thời gian, vstore size.
3. Dedupe: project B clone manifest, seed lock react 18.0.0, thêm libs, `mg install --prefer-dedupe` (reuse 18.0.0 thỏa `^18.0.0` peer) → đếm lại.

Kết quả ghi vào `benchmark_brutal_results_dedupe.md`.

## KPI

- Instance giảm ≥ 20% khi opt-in (mục tiêu tham khảo)
- Install time ≤ +5% overhead
