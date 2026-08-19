# `cli/src/commands/core/create` — Scaffolding Delegation Engine

Module điều phối lệnh khởi tạo dự án (`mg create` và `mg create-<core>`) cho toàn bộ 9 hệ sinh thái của MegaGate.

---

## 🎯 Cấu trúc Thư mục

```
create/
├── mod.rs          # Router phân phối lệnh theo từng core
├── README.md       # Hướng dẫn kiến trúc & cách chạy
├── test/           # Thư mục chứa toàn bộ test cases riêng
│   ├── ai.rs       # Test khởi tạo & route cho AI
│   └── game.rs     # Test khởi tạo các engine Game (Bevy, Godot, Unity, Unreal)
├── ai.rs           # Khởi tạo AI (Python Agent, MCP Server, LangChain)
├── app.rs          # Khởi tạo App (Flutter, React Native, Kotlin, Swift, Tauri, Spring Boot...)
├── cicd.rs         # Khởi tạo CI/CD (GitHub Actions, ArgoCD)
├── clo.rs          # Khởi tạo Cloud IaC (Terraform, CDK, Pulumi, Go APIs)
├── game.rs         # Khởi tạo Game (Bevy, Godot, Unity, Unreal, Raylib)
├── hardware.rs     # Khởi tạo Hardware & Optimizer profiles
├── iot.rs          # Khởi tạo IoT (ESP32-Rust, PlatformIO, Zephyr)
├── library.rs      # Khởi tạo Thư viện đa ngôn ngữ (TS, Rust, Python, Go, Java)
└── web.rs          # Khởi tạo Web Frontend / Backend (Vite, Next.js, NestJS, Spring Boot, Django...)
```

---

## 🚀 Cách Chạy Lệnh CLI

### 1. Web Core (FE / BE / Fullstack)
```bash
# Frontend
mg create-web vite my-vite-app -- --template react-ts
# Fullstack Next.js
mg create-web nextjs@latest my-next-app --ts --tailwind
# Backend NestJS / Spring Boot / Django
mg create-web nestjs my-nest-api
mg create-web spring-boot my-spring-api
mg create-web django my-django-api
```

### 2. App Core (Mobile / Desktop / Backend)
```bash
# Flutter
mg create-app flutter my_mobile_app --org com.megagate
# Desktop Tauri
mg create-app tauri my_desktop_app
# Kotlin Android / Swift iOS
mg create-app kotlin my_android_app
mg create-app swift my_ios_app
```

### 3. AI Core
```bash
mg create-ai mcp-server my-mcp-server
mg create-ai python-agent my-agent
```

### 4. Game Core
```bash
mg create-game bevy my-bevy-game
mg create-game godot my-godot-game
```

---

## 🧪 Hướng Dẫn Chạy Test

Chạy test riêng cho module create:
```bash
cargo test -p mg --bin mg commands::core::create
```
Chạy toàn bộ test CLI:
```bash
cargo test -p mg
```
