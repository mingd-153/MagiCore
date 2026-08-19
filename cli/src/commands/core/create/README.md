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

## 🚀 Hướng Dẫn Chi Tiết Lệnh Tạo Dự Án (9 Cores & Đa Ngôn Ngữ)

### 1. 🌐 Web Core (FE, BE Đa Ngôn Ngữ & Fullstack)
```bash
# Frontend (React, Vue, Svelte, Astro qua Vite/Astro)
mg create-web vite my-react-app -- --template react-ts
mg create-web astro my-astro-blog

# Backend Đa Ngôn Ngữ:
mg create-web spring-boot my-java-api      # Java (Spring Boot)
mg create-web django my-django-api         # Python (Django)
mg create-web fastapi my-fastapi-api       # Python (FastAPI)
mg create-web dotnet my-dotnet-api         # C# (.NET WebAPI)
mg create-web gin my-go-api                # Go (Gin)
mg create-web actix my-rust-api            # Rust (Actix-web)
mg create-web nestjs my-nest-api           # Node/TS (NestJS)

# Fullstack:
mg create-web nextjs@latest my-next-app --ts --tailwind
mg create-web nuxt my-nuxt-app
```

### 2. 📱 App Core (Mobile, Desktop, Backend)
```bash
# Mobile Client (Flutter, React Native, Native Kotlin/Swift)
mg create-app flutter my_flutter_app --org com.megagate
mg create-app react-native my_rn_app
mg create-app kotlin my_android_app
mg create-app swift my_ios_app

# Desktop / Cross-Platform GUI
mg create-app tauri my_tauri_desktop
mg create-app maui my_dotnet_maui_app

# App Backend Services
mg create-app spring-boot my_app_backend
mg create-app ktor my_ktor_backend
mg create-app go-grpc my_grpc_backend
```

### 3. 🤖 AI Core (Agent & Model Context Protocol)
```bash
mg create-ai mcp-server my-mcp-server
mg create-ai python-agent my-agent
mg create-ai langchain-app my-ai-app
```

### 4. 🎮 Game Core (Bevy, Godot, Unity, Unreal)
```bash
mg create-game bevy my-bevy-game --2d       # Rust (Bevy)
mg create-game godot my-godot-game          # Godot Engine
mg create-game unity my-unity-game          # Unity (C#)
mg create-game unreal my-unreal-game        # Unreal (C++)
```

### 5. ☁️ Cloud Core (IaC & Cloud Backends)
```bash
mg create-clo cdk my-cdk-infra --language typescript
mg create-clo pulumi my-pulumi-infra --template aws-typescript
mg create-clo terraform my-tf-infra
mg create-clo gin-go my-cloud-microservice
mg create-clo cloudflare my-worker
```

### 6. 🔌 IoT / Embedded Core (Rust, PlatformIO, Zephyr)
```bash
mg create-iot esp32 my-esp-device --board esp32c3
mg create-iot platformio my-arduino-node --board uno
mg create-iot zephyr my-arm-firmware
```

### 7. 🔄 CI/CD Core
```bash
mg create-cicd github-actions
mg create-cicd argocd
```

### 8. 📚 Lib Core (Thư Viện Đa Ngôn Ngữ)
```bash
mg create-lib ts my-typescript-lib
mg create-lib rust my-rust-crate
mg create-lib python my-python-package
mg create-lib go my-go-module
mg create-lib java my-java-library
mg create-lib dotnet my-dotnet-classlib
```

### 9. ⚙️ Hardware Core
```bash
mg create-hardware bench-profile my-hw-profile
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
