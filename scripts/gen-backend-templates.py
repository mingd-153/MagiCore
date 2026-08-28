#!/usr/bin/env python3
"""Generate feature-gated backend templates for all frameworks."""
import os

BASE = "/Users/doanmihh/Documents/Workspace/MagiCore/templates/web"

# === Source templates ===

NODE_SOURCES = {
    "server.ts": """import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
app.listen(port, () => console.log(`Server running on :${port}`));
""",
    "server.js": """import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
app.listen(port, () => console.log(`Server running on :${port}`));
""",
    "app.ts": """import express from "express";

export const app = express();
app.use(express.json());
app.get("/api/health", (_req, res) => res.json({ status: "ok" }));
""",
    "app.js": """import express from "express";

export const app = express();
app.use(express.json());
app.get("/api/health", (_req, res) => res.json({ status: "ok" }));
""",
    "package.ts.json": """{
  "name": "@project/backend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc",
    "start": "node dist/server.js"
  }
}
""",
    "package.js.json": """{
  "name": "@project/backend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "node --watch src/server.js",
    "start": "node src/server.js"
  }
}
""",
    "tsconfig.json": """{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "dist"
  },
  "include": ["src"]
}
""",
    ".dockerignore": """node_modules
dist
.git
.env
""",
    "Dockerfile": """FROM node:22-alpine AS builder
WORKDIR /app
COPY package*.json tsconfig.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM node:22-alpine
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY package*.json ./
EXPOSE 3000
CMD ["node", "dist/server.js"]
""",
}

FASTIFY_TS_SERVER = """import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
await app.listen({ host: "127.0.0.1", port });
console.log(`Server running on :${port}`);
"""

FASTIFY_JS_SERVER = """import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
await app.listen({ host: "127.0.0.1", port });
console.log(`Server running on :${port}`);
"""

FASTIFY_TS_APP = """import Fastify from "fastify";

export const app = Fastify({ logger: false });

app.get("/health", async () => ({ status: "ok" }));
"""

FASTIFY_JS_APP = """import Fastify from "fastify";

export const app = Fastify({ logger: false });

app.get("/health", async () => ({ status: "ok" }));
"""

HONO_TS_SERVER = """import { serve } from "@hono/node-server";
import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
serve({ fetch: app.fetch, port });
console.log(`Server running on :${port}`);
"""

HONO_JS_SERVER = """import { serve } from "@hono/node-server";
import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
serve({ fetch: app.fetch, port });
console.log(`Server running on :${port}`);
"""

HONO_TS_APP = """import { Hono } from "hono";

export const app = new Hono();

app.get("/health", (c) => c.json({ status: "ok" }));
"""

HONO_JS_APP = """import { Hono } from "hono";

export const app = new Hono();

app.get("/health", (c) => c.json({ status: "ok" }));
"""

TRPC_TS_APP = """import express from "express";
import { createExpressMiddleware } from "@trpc/server/adapters/express";
import { appRouter } from "./router.js";
import { createContext } from "./context.js";

export const app = express();

app.use("/trpc", createExpressMiddleware({ router: appRouter, createContext }));
app.get("/health", (_req, res) => res.json({ status: "ok" }));
"""

TRPC_JS_APP = """import express from "express";
import { createExpressMiddleware } from "@trpc/server/adapters/express";
import { appRouter } from "./router.js";
import { createContext } from "./context.js";

export const app = express();

app.use("/trpc", createExpressMiddleware({ router: appRouter, createContext }));
app.get("/health", (_req, res) => res.json({ status: "ok" }));
"""

TRPC_TS_TRPC = """import { initTRPC } from "@trpc/server";

const t = initTRPC.create();

export const router = t.router;
export const publicProcedure = t.procedure;
"""

TRPC_JS_TRPC = """import { initTRPC } from "@trpc/server";

const t = initTRPC.create();

export const router = t.router;
export const publicProcedure = t.procedure;
"""

TRPC_TS_ROUTER = """import { z } from "zod";
import { router, publicProcedure } from "./trpc.js";

export const appRouter = router({
  greeting: publicProcedure
    .input(z.object({ name: z.string().optional() }).optional())
    .query(({ input }) => `hello ${input?.name ?? "magicore"}`),
});

export type AppRouter = typeof appRouter;
"""

TRPC_JS_ROUTER = """import { z } from "zod";
import { router, publicProcedure } from "./trpc.js";

export const appRouter = router({
  greeting: publicProcedure
    .input(z.object({ name: z.string().optional() }).optional())
    .query(({ input }) => `hello ${input?.name ?? "magicore"}`),
});
"""

TRPC_TS_CONTEXT = """export async function createContext() {
  return {};
}
"""

TRPC_JS_CONTEXT = """export async function createContext() {
  return {};
}
"""

NESTJS_PACKAGE_TS = """{
  "name": "@project/backend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc",
    "start": "node dist/server.js"
  }
}
"""

NESTJS_PACKAGE_JS = """{
  "name": "@project/backend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc",
    "start": "node dist/server.js"
  }
}
"""

NESTJS_TSCONFIG = """{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true,
    "outDir": "dist"
  },
  "include": ["src"]
}
"""

NESTJS_SERVER_TS = """import "reflect-metadata";
import { NestFactory } from "@nestjs/core";
import { AppModule } from "./app.module.js";

const port = parseInt(process.env.PORT || "3000", 10);
const app = await NestFactory.create(AppModule);
await app.listen(port);
console.log(`Server running on :${port}`);
"""

NESTJS_SERVER_JS = """import "reflect-metadata";
import { NestFactory } from "@nestjs/core";
import { AppModule } from "./app.module.js";

const port = parseInt(process.env.PORT || "3000", 10);
const app = await NestFactory.create(AppModule);
await app.listen(port);
console.log(`Server running on :${port}`);
"""

NESTJS_APP_MODULE_TS = """import { Module } from "@nestjs/common";
import { AppController } from "./app.controller.js";
import { AppService } from "./app.service.js";

@Module({
  imports: [],
  controllers: [AppController],
  providers: [AppService],
})
export class AppModule {}
"""

NESTJS_APP_MODULE_JS = """import { Module } from "@nestjs/common";
import { AppController } from "./app.controller.js";
import { AppService } from "./app.service.js";

@Module({
  imports: [],
  controllers: [AppController],
  providers: [AppService],
})
export class AppModule {}
"""

NESTJS_APP_CONTROLLER_TS = """import { Controller, Get } from "@nestjs/common";

@Controller()
export class AppController {
  @Get("/health")
  health() {
    return { status: "ok" };
  }
}
"""

NESTJS_APP_CONTROLLER_JS = """import { Controller, Get } from "@nestjs/common";

@Controller()
export class AppController {
  @Get("/health")
  health() {
    return { status: "ok" };
  }
}
"""

NESTJS_APP_SERVICE_TS = """import { Injectable } from "@nestjs/common";

@Injectable()
export class AppService {
  health() {
    return { status: "ok" };
  }
}
"""

NESTJS_APP_SERVICE_JS = """import { Injectable } from "@nestjs/common";

@Injectable()
export class AppService {
  health() {
    return { status: "ok" };
  }
}
"""

NESTJS_TEST_TS = """import { describe, it, expect } from "vitest";
import { AppModule } from "../src/app.module.js";

describe("AppModule", () => {
  it("exports module", () => {
    expect(AppModule).toBeDefined();
  });
});
"""

NODE_TEST_SOURCES = {
    "vitest.config.ts": """import { defineConfig } from "vitest/config";

export default defineConfig({ test: { globals: true } });
""",
    "app.spec.ts": """import { describe, it, expect } from "vitest";
import { app } from "../src/lib/app.js";

describe("app", () => {
  it("exports app", () => { expect(app).toBeDefined(); });
});
""",
}

NODE_ORM_SOURCES = {
    "schema.prisma": """generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}
""",
    "prisma.ts": """import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();
export default prisma;
""",
}

GO_SOURCES = {
    "main.go": """package main

import (
  "log"
  "net/http"
  "os"
)

func main() {
  port := os.Getenv("PORT")
  if port == "" { port = "3000" }
  http.HandleFunc("/api/health", func(w http.ResponseWriter, r *http.Request) {
    w.Header().Set("Content-Type", "application/json")
    w.Write([]byte(`{"status":"ok"}`))
  })
  log.Printf("Listening on :%s", port)
  log.Fatal(http.ListenAndServe(":"+port, nil))
}
""",
    "go.mod": """module github.com/{{ project_slug }}/api
go 1.22
""",
    ".dockerignore": """build/
dist/
.git
.env
""",
    "Dockerfile": """FROM golang:1.22-alpine AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o /server

FROM alpine:3.19
WORKDIR /app
COPY --from=builder /server .
EXPOSE 3000
CMD ["./server"]
""",
}

PYTHON_SOURCES = {
    "main.py": """from fastapi import FastAPI
import uvicorn

app = FastAPI()

@app.get("/api/health")
async def health():
    return {"status": "ok"}

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=int(os.getenv("PORT", "3000")))
""",
    "requirements.txt": """fastapi==0.111.0
uvicorn[standard]==0.29.0
""",
    ".dockerignore": """__pycache__/
*.pyc
.venv/
.git
.env
""",
    "Dockerfile": """FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
EXPOSE 3000
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "3000"]
""",
}

RUST_SOURCES = {
    "main.rs": """use axum::{Router, routing::get, Json};
use std::net::SocketAddr;
use serde_json::json;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/api/health", get(|| async { Json(json!({"status":"ok"})) }));
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
""",
    "Cargo.toml": """[package]
name = "{{ project_slug }}-api"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
""",
    ".dockerignore": """target/
.git
.env
""",
    "Dockerfile": """FROM rust:1.77-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/{{ project_slug }}-api /app/server
EXPOSE 3000
CMD ["./server"]
""",
}

def ensure_dir(path):
    os.makedirs(path, exist_ok=True)

def write_file(path, content):
    ensure_dir(os.path.dirname(path))
    with open(path, "w") as f:
        f.write(content.lstrip("\n"))

# === Node backend templates ===
NODE_FWS = ["express", "fastify", "nestjs", "hono", "trpc"]
NODE_FEATURES = ["eslint", "prettier", "vitest", "jest", "prisma", "drizzle", "docker"]

def make_node_toml(fw):
    if fw == "nestjs":
        return """[[files]]
source = "package.ts.json"
target = "package.json"
required_context = ["project_slug"]
include_features = ["typescript"]

[[files]]
source = "package.js.json"
target = "package.json"
required_context = ["project_slug"]
exclude_features = ["typescript"]

[[files]]
source = "tsconfig.json"
target = "tsconfig.json"

[[files]]
source = "server.ts"
target = "src/server.ts"

[[files]]
source = "server.js"
target = "src/server.js"
exclude_features = ["typescript"]

[[files]]
source = "app.module.ts"
target = "src/app.module.ts"

[[files]]
source = "app.module.js"
target = "src/app.module.js"
exclude_features = ["typescript"]

[[files]]
source = "app.controller.ts"
target = "src/app.controller.ts"

[[files]]
source = "app.controller.js"
target = "src/app.controller.js"
exclude_features = ["typescript"]

[[files]]
source = "app.service.ts"
target = "src/app.service.ts"

[[files]]
source = "app.service.js"
target = "src/app.service.js"
exclude_features = ["typescript"]

[[files]]
source = "Dockerfile"
target = "Dockerfile"
include_features = ["docker"]

[[files]]
source = ".dockerignore"
target = ".dockerignore"
include_features = ["docker"]

[[files]]
source = "vitest.config.ts"
target = "vitest.config.ts"
include_features = ["vitest"]

[[files]]
source = "app.spec.ts"
target = "src/app.spec.ts"
include_features = ["vitest"]
"""
    parts = []
    # Core files with TS/JS gating
    for ext in ["ts", "js"]:
        inc = f'\ninclude_features = ["typescript"]' if ext == "ts" else f'\nexclude_features = ["typescript"]'
        parts.append(f"""[[files]]
source = "package.{ext}.json"
target = "package.json"
required_context = ["project_slug"]{inc}
""")
    parts.append(f"""[[files]]
source = "tsconfig.json"
target = "tsconfig.json"
include_features = ["typescript"]
""")
    for src in ["server", "app"]:
        parts.append(f"""[[files]]
source = "{src}.js"
target = "src/{src}.js"
exclude_features = ["typescript"]

[[files]]
source = "{src}.ts"
target = "src/{src}.ts"
include_features = ["typescript"]
""")
    # Docker
    parts.append(f"""[[files]]
source = "Dockerfile"
target = "Dockerfile"
include_features = ["docker"]

[[files]]
source = ".dockerignore"
target = ".dockerignore"
include_features = ["docker"]
""")
    # Testing
    parts.append(f"""[[files]]
source = "vitest.config.ts"
target = "vitest.config.ts"
include_features = ["vitest"]

[[files]]
source = "app.spec.ts"
target = "src/app.spec.ts"
include_features = ["vitest"]
""")
    # ORM
    parts.append(f"""[[files]]
source = "schema.prisma"
target = "prisma/schema.prisma"
include_features = ["prisma"]

[[files]]
source = "prisma.ts"
target = "src/lib/prisma.ts"
include_features = ["prisma"]
""")
    if fw == "trpc":
        parts.append("""[[files]]
source = "trpc.ts"
target = "src/trpc.ts"
include_features = ["typescript"]

[[files]]
source = "trpc.js"
target = "src/trpc.js"
exclude_features = ["typescript"]

[[files]]
source = "router.ts"
target = "src/router.ts"
include_features = ["typescript"]

[[files]]
source = "router.js"
target = "src/router.js"
exclude_features = ["typescript"]

[[files]]
source = "context.ts"
target = "src/context.ts"
include_features = ["typescript"]

[[files]]
source = "context.js"
target = "src/context.js"
exclude_features = ["typescript"]
""")
    return "\n".join(parts)

for fw in NODE_FWS:
    d = f"{BASE}/backend/node/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in NODE_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    if fw == "fastify":
        write_file(f"{d}/sources/server.ts", FASTIFY_TS_SERVER)
        write_file(f"{d}/sources/server.js", FASTIFY_JS_SERVER)
        write_file(f"{d}/sources/app.ts", FASTIFY_TS_APP)
        write_file(f"{d}/sources/app.js", FASTIFY_JS_APP)
    if fw == "hono":
        write_file(f"{d}/sources/server.ts", HONO_TS_SERVER)
        write_file(f"{d}/sources/server.js", HONO_JS_SERVER)
        write_file(f"{d}/sources/app.ts", HONO_TS_APP)
        write_file(f"{d}/sources/app.js", HONO_JS_APP)
    if fw == "trpc":
        write_file(f"{d}/sources/app.ts", TRPC_TS_APP)
        write_file(f"{d}/sources/app.js", TRPC_JS_APP)
        write_file(f"{d}/sources/trpc.ts", TRPC_TS_TRPC)
        write_file(f"{d}/sources/trpc.js", TRPC_JS_TRPC)
        write_file(f"{d}/sources/router.ts", TRPC_TS_ROUTER)
        write_file(f"{d}/sources/router.js", TRPC_JS_ROUTER)
        write_file(f"{d}/sources/context.ts", TRPC_TS_CONTEXT)
        write_file(f"{d}/sources/context.js", TRPC_JS_CONTEXT)
    if fw == "nestjs":
        write_file(f"{d}/sources/package.ts.json", NESTJS_PACKAGE_TS)
        write_file(f"{d}/sources/package.js.json", NESTJS_PACKAGE_JS)
        write_file(f"{d}/sources/tsconfig.json", NESTJS_TSCONFIG)
        write_file(f"{d}/sources/server.ts", NESTJS_SERVER_TS)
        write_file(f"{d}/sources/server.js", NESTJS_SERVER_JS)
        write_file(f"{d}/sources/app.module.ts", NESTJS_APP_MODULE_TS)
        write_file(f"{d}/sources/app.module.js", NESTJS_APP_MODULE_JS)
        write_file(f"{d}/sources/app.controller.ts", NESTJS_APP_CONTROLLER_TS)
        write_file(f"{d}/sources/app.controller.js", NESTJS_APP_CONTROLLER_JS)
        write_file(f"{d}/sources/app.service.ts", NESTJS_APP_SERVICE_TS)
        write_file(f"{d}/sources/app.service.js", NESTJS_APP_SERVICE_JS)
    for name, content in NODE_TEST_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    if fw == "nestjs":
        write_file(f"{d}/sources/app.spec.ts", NESTJS_TEST_TS)
    for name, content in NODE_ORM_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    with open(f"{d}/template.toml", "w") as f:
        f.write(make_node_toml(fw))
    print(f"  node/{fw} done")

# === Non-Node backend templates (minimal + docker) ===
def make_be_toml(files_list, lang):
    """Generate template.toml for non-Node backends (just source + docker)."""
    parts = []
    for src, target in files_list:
        parts.append(f"""[[files]]
source = "{src}"
target = "{target}"
""")
    # Docker
    parts.append(f"""[[files]]
source = "Dockerfile"
target = "Dockerfile"
include_features = ["docker"]

[[files]]
source = ".dockerignore"
target = ".dockerignore"
include_features = ["docker"]
""")
    return "\n".join(parts)

# Go backends
GO_FWS = ["gin", "echo", "fiber"]
GO_FILES = [("go.mod", "go.mod"), ("main.go", "cmd/server/main.go")]
for fw in GO_FWS:
    d = f"{BASE}/backend/go/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in GO_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    with open(f"{d}/template.toml", "w") as f:
        f.write(make_be_toml(GO_FILES, "go"))
    print(f"  go/{fw} done")

# Python backends
PY_FWS = ["fastapi", "flask"]
PY_FILES = [("main.py", "src/main.py"), ("requirements.txt", "requirements.txt")]
for fw in PY_FWS:
    d = f"{BASE}/backend/python/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in PYTHON_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    with open(f"{d}/template.toml", "w") as f:
        f.write(make_be_toml(PY_FILES, "python"))
    print(f"  python/{fw} done")

# Django (special - has manage.py structure)
DJANGO_SOURCES = PYTHON_SOURCES.copy()
DJANGO_SOURCES["manage.py"] = '#!/usr/bin/env python\n"""Django command-line utility for administrative tasks."""\nimport os\nimport sys\n\nos.environ.setdefault("DJANGO_SETTINGS_MODULE", "config.settings")\n\nif __name__ == "__main__":\n    from django.core.management import execute_from_command_line\n    execute_from_command_line(sys.argv)\n'
d = f"{BASE}/backend/python/django"
ensure_dir(f"{d}/sources")
for name, content in {**DJANGO_SOURCES, **{k: v for k, v in PYTHON_SOURCES.items()}}.items():
    write_file(f"{d}/sources/{name}", content)
with open(f"{d}/template.toml", "w") as f:
    f.write(make_be_toml([("manage.py", "manage.py"), ("main.py", "src/main.py"), ("requirements.txt", "requirements.txt")], "python"))
print("  python/django done")

# Java backends
JAVA_SOURCES = {
    "pom.xml": """<project xmlns="http://maven.apache.org/POM/4.0.0"
  xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <modelVersion>4.0.0</modelVersion>
  <parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId><version>3.3.0</version></parent>
  <groupId>com.{{ project_slug }}</groupId>
  <artifactId>api</artifactId>
  <version>0.1.0</version>
  <properties><java.version>21</java.version></properties>
  <dependencies>
    <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency>
  </dependencies>
</project>
""",
    "Application.java": """package com.{{ project_slug }};

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class Application {
    public static void main(String[] args) {
        SpringApplication.run(Application.class, args);
    }
}
""",
    "HealthController.java": """package com.{{ project_slug }}.api;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;
import java.util.Map;

@RestController
public class HealthController {
    @GetMapping("/api/health")
    public Map<String, String> health() {
        return Map.of("status", "ok");
    }
}
""",
    ".dockerignore": """target/
.git
.env
""",
    "Dockerfile": """FROM maven:3.9-eclipse-temurin-21 AS builder
WORKDIR /app
COPY pom.xml ./
RUN mvn dependency:go-offline
COPY src ./src
RUN mvn package -DskipTests

FROM eclipse-temurin:21-jre-alpine
WORKDIR /app
COPY --from=builder /app/target/*.jar app.jar
EXPOSE 3000
CMD ["java", "-jar", "app.jar"]
""",
}
JAVA_FILES = [("pom.xml", "pom.xml"), ("Application.java", "src/main/java/com/{{ project_slug }}/Application.java"),
              ("HealthController.java", "src/main/java/com/{{ project_slug }}/api/HealthController.java")]

for fw in ["spring-boot", "quarkus"]:
    d = f"{BASE}/backend/java/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in JAVA_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    with open(f"{d}/template.toml", "w") as f:
        f.write(make_be_toml(JAVA_FILES, "java"))
    print(f"  java/{fw} done")

# Rust backends
RUST_FWS = ["axum", "actix-web"]
RUST_FILES = [("Cargo.toml", "Cargo.toml"), ("main.rs", "src/main.rs")]
for fw in RUST_FWS:
    d = f"{BASE}/backend/rust/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in RUST_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    with open(f"{d}/template.toml", "w") as f:
        f.write(make_be_toml(RUST_FILES, "rust"))
    print(f"  rust/{fw} done")

# PHP backends
PHP_SOURCES = {
    "index.php": """<?php
require __DIR__ . '/vendor/autoload.php';

use Slim\\Factory\\AppFactory;

$app = AppFactory::create();
$app->get('/api/health', function ($request, $response) {
    $response->getBody()->write(json_encode(['status' => 'ok']));
    return $response->withHeader('Content-Type', 'application/json');
});
$app->run();
""",
    "composer.json": """{ "name": "{{ project_slug }}/api", "require": { "slim/slim": "^4", "slim/psr7": "^1" } }
""",
    ".dockerignore": """vendor/
.git
.env
""",
    "Dockerfile": """FROM composer:2 AS composer
WORKDIR /app
COPY composer.json ./
RUN composer install --no-dev

FROM php:8.3-cli-alpine
WORKDIR /app
COPY --from=composer /app/vendor ./vendor
COPY . .
EXPOSE 3000
CMD ["php", "-S", "0.0.0.0:3000", "-t", "/app"]
""",
}
PHP_FILES = [("composer.json", "composer.json"), ("index.php", "public/index.php")]

for fw in ["laravel", "symfony"]:
    d = f"{BASE}/backend/php/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in PHP_SOURCES.items():
        write_file(f"{d}/sources/{name}", content)
    with open(f"{d}/template.toml", "w") as f:
        f.write(make_be_toml(PHP_FILES, "php"))
    print(f"  php/{fw} done")

print("\nAll backend templates generated!")
