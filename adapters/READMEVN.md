# `adapters/` — Các bộ điều hợp hệ sinh thái (9 core)

Mỗi adapter triển khai các trait của `mgc-adapter-base` cho một hệ sinh thái công nghệ cụ thể.

## Tổng quan

| Adapter | Crate | Hệ sinh thái |
|---|---|---|
| `web/` | `mgc-web-adapter` | Node.js, NPM, TypeScript, React, Vue, Next.js, FastAPI, Django, Spring Boot, Gin, Laravel, Symfony… |
| `ai/` | `mgc-ai-adapter` | Python AI/ML, phục vụ LLM, Hugging Face, scaffold framework MCP |
| `cloud/` | `mgc-cloud-adapter` | Terraform, Pulumi, AWS CDK, Cloudflare Workers, Serverless |
| `cicd/` | `mgc-cicd-adapter` | GitHub Actions, GitLab CI, ArgoCD, Docker Compose, Kubernetes |
| `game/` | `mgc-game-adapter` | Godot 4, Unity, Unreal Engine, Bevy (Rust) |
| `iot/` | `mgc-iot-adapter` | PlatformIO, Zephyr RTOS, ESP32, Arduino, STM32 |
| `app/` | `mgc-app-adapter` | Flutter, Swift Package Manager, Kotlin/Gradle, React Native |
| `lib/` | `mgc-lib-adapter` | Thư viện đa ngôn ngữ: Rust crate, Python package và npm package |
| `hardware/` | `mgc-hardware-adapter` | Benchmark phần cứng, phân bổ tài nguyên, nhận diện nền tảng |

## Kiến trúc

Mỗi adapter dùng chung cấu trúc nội bộ sau:

```text
adapters/<name>/
├── Cargo.toml
└── src/
    ├── lib.rs          # Re-export và API công khai
    ├── install.rs      # Logic cài package
    ├── manifest.rs     # Đọc manifest (package.json / mgc.toml / ...)
    ├── scaffold.rs     # Template khởi tạo dự án
    ├── audit.rs        # Kiểm tra cảnh báo bảo mật
    └── ...
```

## `web/` — Adapter tham chiếu

Web là adapter trưởng thành nhất và làm implementation tham chiếu cho các adapter khác. Chức năng được mô tả tại [`web/README.md`](web/README.md); mức hỗ trợ cụ thể còn phụ thuộc framework và thao tác.

## Thêm adapter

1. Dùng `adapters/lib/` làm điểm khởi đầu.
2. Triển khai các trait trong `core/crates/mgc-adapter-base/`.
3. Đăng ký adapter trong `cli/src/dispatch/per_core.rs`.
4. Thêm integration test trong `adapters/<name>/tests/`.
