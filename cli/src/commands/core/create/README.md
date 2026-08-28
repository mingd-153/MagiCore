# `cli/src/commands/core/create` — Scaffolding Delegation Engine

Module điều phối lệnh khởi tạo dự án (`mgc create` và `mgc create-<core>`) cho toàn bộ 9 hệ sinh thái của MagiCore.

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
mgc create-web vite my-react-app -- --template react-ts
mgc create-web astro my-astro-blog

# Backend Đa Ngôn Ngữ:
mgc create-web spring-boot my-java-api      # Java (Spring Boot)
mgc create-web django my-django-api         # Python (Django)
mgc create-web fastapi my-fastapi-api       # Python (FastAPI)
mgc create-web dotnet my-dotnet-api         # C# (.NET WebAPI)
mgc create-web gin my-go-api                # Go (Gin)
mgc create-web actix my-rust-api            # Rust (Actix-web)
mgc create-web nestjs my-nest-api           # Node/TS (NestJS)

# Fullstack:
mgc create-web nextjs@latest my-next-app --ts --tailwind
mgc create-web nuxt my-nuxt-app
```

### 2. 📱 App Core (Mobile, Desktop, Backend)
```bash
# Mobile Client (Flutter, React Native, Native Kotlin/Swift)
mgc create-app flutter my_flutter_app --org com.magicore
mgc create-app react-native my_rn_app
mgc create-app kotlin my_android_app
mgc create-app swift my_ios_app

# Desktop / Cross-Platform GUI
mgc create-app tauri my_tauri_desktop
mgc create-app maui my_dotnet_maui_app

# App Backend Services
mgc create-app spring-boot my_app_backend
mgc create-app ktor my_ktor_backend
mgc create-app go-grpc my_grpc_backend
```

### 3. 🤖 AI Core (Agent & Model Context Protocol)
```bash
mgc create-ai mcp-server my-mcp-server
mgc create-ai python-agent my-agent
mgc create-ai langchain-app my-ai-app
```

### 4. 🎮 Game Core (Bevy, Godot, Unity, Unreal)
```bash
mgc create-game bevy my-bevy-game --2d       # Rust (Bevy)
mgc create-game godot my-godot-game          # Godot Engine
mgc create-game unity my-unity-game          # Unity (C#)
mgc create-game unreal my-unreal-game        # Unreal (C++)
```

### 5. ☁️ Cloud Core (IaC & Cloud Backends)
```bash
mgc create-clo cdk my-cdk-infra --language typescript
mgc create-clo pulumi my-pulumi-infra --template aws-typescript
mgc create-clo terraform my-tf-infra
mgc create-clo gin-go my-cloud-microservice
mgc create-clo cloudflare my-worker
```

### 6. 🔌 IoT / Embedded Core (Rust, PlatformIO, Zephyr)
```bash
mgc create-iot esp32 my-esp-device --board esp32c3
mgc create-iot platformio my-arduino-node --board uno
mgc create-iot zephyr my-arm-firmware
```

### 7. 🔄 CI/CD Core
```bash
mgc create-cicd github-actions
mgc create-cicd argocd
```

### 8. 📚 Lib Core (Thư Viện Đa Ngôn Ngữ)
```bash
mgc create-lib ts my-typescript-lib
mgc create-lib rust my-rust-crate
mgc create-lib python my-python-package
mgc create-lib go my-go-module
mgc create-lib java my-java-library
mgc create-lib dotnet my-dotnet-classlib
```

### 9. ⚙️ Hardware Core
```bash
mgc create-hardware bench-profile my-hw-profile
```

---

## 🧪 Hướng Dẫn Chạy Test

Chạy test riêng cho module create:
```bash
cargo test -p mgc --bin mgc commands::core::create
```
Chạy toàn bộ test CLI:
```bash
cargo test -p mgc
```
