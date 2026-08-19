//! In-Memory Template Kernel (Học từ Bun `SourceFileProjectGenerator.rs`)
//!
//! Lưu trữ template core trực tiếp trong binary qua `include_bytes!`/const string.
//! Thực hiện zero-disk in-memory string replacement để sinh dự án cực nhanh (<50ms)
//! mà không cần kết nối mạng.

use anyhow::Result;
use std::path::Path;

/// Core embedded template file definition
pub struct EmbeddedFile {
    pub path: &'static str,
    pub content: &'static str,
}

/// Lấy danh sách files template embedded theo framework
pub fn get_embedded_template(core: &str, framework: &str) -> Option<Vec<EmbeddedFile>> {
    match (core, framework) {
        ("web", "react") | ("web", "react-vite") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.7.2",
    "vite": "^6.0.0"
  }
}"#,
            },
            EmbeddedFile {
                path: "index.html",
                content: r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{{PROJECT_NAME}}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>"#,
            },
            EmbeddedFile {
                path: "src/main.tsx",
                content: r#"import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)"#,
            },
            EmbeddedFile {
                path: "src/App.tsx",
                content: r#"export default function App() {
  return (
    <div style={{ padding: '2rem', fontFamily: 'sans-serif' }}>
      <h1>Welcome to {{PROJECT_NAME}}</h1>
      <p>Powered by MegaGate & React Vite</p>
    </div>
  )
}"#,
            },
            EmbeddedFile {
                path: "vite.config.ts",
                content: r#"import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
})"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"]
}"#,
            },
        ]),
        ("web", "axum") => Some(vec![
            EmbeddedFile {
                path: "Cargo.toml",
                content: r#"[package]
name = "{{PROJECT_NAME}}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
"#,
            },
            EmbeddedFile {
                path: "src/main.rs",
                content: r#"use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello from MegaGate Axum!" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4315").await.unwrap();
    println!("Listening on http://127.0.0.1:4315");
    axum::serve(listener, app).await.unwrap();
}
"#,
            },
        ]),
        ("web", "fastapi") => Some(vec![
            EmbeddedFile {
                path: "main.py",
                content: r#"from fastapi import FastAPI

app = FastAPI(title="{{PROJECT_NAME}}")

@app.get("/")
def read_root():
    return {"message": "Hello from MegaGate FastAPI!", "project": "{{PROJECT_NAME}}"}
"#,
            },
            EmbeddedFile {
                path: "pyproject.toml",
                content: r#"[project]
name = "{{PROJECT_NAME}}"
version = "0.1.0"
dependencies = [
    "fastapi>=0.110.0",
    "uvicorn>=0.28.0",
]
"#,
            },
        ]),
        _ => None,
    }
}

/// Materialize template files ra thư mục đích với in-memory string replacement
pub fn materialize_embedded(
    target_dir: &Path,
    project_name: &str,
    files: &[EmbeddedFile],
) -> Result<()> {
    std::fs::create_dir_all(target_dir)?;
    for file in files {
        let dest = target_dir.join(file.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // In-memory string replacement không qua disk buffer
        let rendered = file.content.replace("{{PROJECT_NAME}}", project_name);
        std::fs::write(&dest, rendered)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/embedded_kernel.rs"]
mod tests;
