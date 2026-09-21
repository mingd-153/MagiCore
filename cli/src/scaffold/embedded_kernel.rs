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
    "preview": "vite preview --port 4315 --strictPort",
    "start": "vite preview --port 4315 --strictPort",
    "test": "tsc --noEmit"
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
      <p>Powered by MagiCore & React Vite</p>
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
        ("web", "vue") | ("web", "vue-vite") => Some(vec![
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
    "preview": "vite preview --port 4315 --strictPort",
    "start": "vite preview --port 4315 --strictPort",
    "test": "vue-tsc --noEmit"
  },
  "dependencies": {
    "vue": "^3.5.0"
  },
  "devDependencies": {
    "@vitejs/plugin-vue": "^5.2.1",
    "typescript": "^5.7.2",
    "vite": "^6.0.0",
    "vue-tsc": "^2.2.0"
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
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>"#,
            },
            EmbeddedFile {
                path: "src/main.ts",
                content: r#"import { createApp } from 'vue'
import App from './App.vue'

createApp(App).mount('#app')
"#,
            },
            EmbeddedFile {
                path: "src/App.vue",
                content: r#"<script setup lang="ts">
const projectName = '{{PROJECT_NAME}}'
</script>

<template>
  <main>
    <h1>Welcome to {{ projectName }}</h1>
    <p>Powered by MagiCore and Vue Vite</p>
  </main>
</template>
"#,
            },
            EmbeddedFile {
                path: "vite.config.ts",
                content: r#"import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
})
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "jsx": "preserve",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "noEmit": true
  },
  "include": ["src/**/*.ts", "src/**/*.vue"]
}"#,
            },
        ]),
        ("web", "express") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc",
    "start": "node dist/server.js"
  },
  "dependencies": {
    "express": "^5.0.0"
  },
  "devDependencies": {
    "@types/express": "^5.0.0",
    "@types/node": "^22.0.0",
    "tsx": "^4.19.0",
    "typescript": "^5.7.2"
  }
}"#,
            },
            EmbeddedFile {
                path: "src/server.ts",
                content: r#"import express from 'express'

const app = express()
const port = Number(process.env.PORT ?? 4315)

app.get('/', (_req, res) => {
  res.json({ message: 'Hello from MagiCore Express!', project: '{{PROJECT_NAME}}' })
})

app.listen(port, () => {
  console.log(`Listening on http://127.0.0.1:${port}`)
})
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "dist",
    "skipLibCheck": true
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
    let app = Router::new().route("/", get(|| async { "Hello from MagiCore Axum!" }));
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
    return {"message": "Hello from MagiCore FastAPI!", "project": "{{PROJECT_NAME}}"}
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
        ("web", "nextjs") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "next dev --port 4315",
    "build": "next build",
    "start": "next start --port 4315"
  },
  "dependencies": {
    "next": "^15.0.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "typescript": "^5.7.2"
  }
}"#,
            },
            EmbeddedFile {
                path: "next.config.mjs",
                content: r#"/** @type {import('next').NextConfig} */
const nextConfig = {};
export default nextConfig;
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "preserve",
    "strict": true,
    "plugins": [{ "name": "next" }],
    "paths": { "@/*": ["./*"] }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}"#,
            },
            EmbeddedFile {
                path: "app/layout.tsx",
                content: r#"export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
"#,
            },
            EmbeddedFile {
                path: "app/page.tsx",
                content: r#"export default function Page() {
  return (
    <main style={{ padding: '2rem', fontFamily: 'sans-serif' }}>
      <h1>Welcome to {{PROJECT_NAME}}</h1>
      <p>Powered by MagiCore & Next.js</p>
    </main>
  );
}
"#,
            },
        ]),
        ("web", "nuxt") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "nuxt dev --port 4315",
    "build": "nuxt build",
    "preview": "nuxt preview --port 4315"
  },
  "dependencies": {
    "nuxt": "^3.0.0",
    "vue": "^3.5.0",
    "vue-router": "^4.5.0"
  },
  "devDependencies": {
    "typescript": "^5.7.2"
  }
}"#,
            },
            EmbeddedFile {
                path: "nuxt.config.ts",
                content: r#"export default defineNuxtConfig({
  devtools: { enabled: false },
});
"#,
            },
            EmbeddedFile {
                path: "app.vue",
                content: r#"<template>
  <main style="padding: 2rem; font-family: sans-serif">
    <h1>Welcome to {{PROJECT_NAME}}</h1>
    <p>Powered by MagiCore & Nuxt</p>
  </main>
</template>
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "extends": "./.nuxt/tsconfig.json"
}"#,
            },
        ]),
        ("web", "sveltekit") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite dev --port 4315",
    "build": "vite build",
    "preview": "vite preview --port 4315"
  },
  "devDependencies": {
    "@sveltejs/adapter-auto": "^3.0.0",
    "@sveltejs/kit": "^2.0.0",
    "@sveltejs/vite-plugin-svelte": "^4.0.0",
    "svelte": "^5.0.0",
    "typescript": "^5.7.2",
    "vite": "^6.0.0"
  }
}"#,
            },
            EmbeddedFile {
                path: "svelte.config.js",
                content: r#"import adapter from '@sveltejs/adapter-auto';

/** @type {import('@sveltejs/kit').Config} */
const config = { kit: { adapter: adapter() } };
export default config;
"#,
            },
            EmbeddedFile {
                path: "vite.config.ts",
                content: r#"import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({ plugins: [sveltekit()] });
"#,
            },
            EmbeddedFile {
                path: "src/app.html",
                content: r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    %sveltekit.head%
  </head>
  <body data-sveltekit-preload-data="hover">
    <div style="display: contents">%sveltekit.body%</div>
  </body>
</html>
"#,
            },
            EmbeddedFile {
                path: "src/routes/+page.svelte",
                content: r#"<main style="padding: 2rem; font-family: sans-serif">
  <h1>Welcome to {{PROJECT_NAME}}</h1>
  <p>Powered by MagiCore & SvelteKit</p>
</main>
"#,
            },
        ]),
        ("web", "angular") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "scripts": {
    "ng": "ng",
    "start": "ng serve --port 4315",
    "build": "ng build",
    "test": "tsc --noEmit -p tsconfig.app.json"
  },
  "dependencies": {
    "@angular/common": "^19.0.0",
    "@angular/compiler": "^19.0.0",
    "@angular/core": "^19.0.0",
    "@angular/platform-browser": "^19.0.0",
    "rxjs": "^7.8.0",
    "tslib": "^2.8.0",
    "zone.js": "^0.15.0"
  },
  "devDependencies": {
    "@angular-devkit/build-angular": "^19.0.0",
    "@angular/cli": "^19.0.0",
    "@angular/compiler-cli": "^19.0.0",
    "typescript": "~5.6.0"
  }
}"#,
            },
            EmbeddedFile {
                path: "angular.json",
                content: r#"{
  "$schema": "./node_modules/@angular/cli/lib/config/schema.json",
  "version": 1,
  "newProjectRoot": "projects",
  "projects": {
    "{{PROJECT_NAME}}": {
      "projectType": "application",
      "root": "",
      "sourceRoot": "src",
      "architect": {
        "build": {
          "builder": "@angular-devkit/build-angular:application",
          "options": {
            "outputPath": "dist/{{PROJECT_NAME}}",
            "index": "src/index.html",
            "browser": "src/main.ts",
            "polyfills": ["zone.js"],
            "tsConfig": "tsconfig.app.json",
            "styles": ["src/styles.css"],
            "ssr": false
          },
          "configurations": {
            "production": {
              "optimization": true,
              "outputHashing": "all",
              "sourceMap": false,
              "namedChunks": false,
              "aot": true,
              "extractLicenses": true,
              "vendorChunk": false,
              "buildOptimizer": true
            }
          }
        },
        "serve": {
          "builder": "@angular-devkit/build-angular:dev-server",
          "options": { "port": 4315 }
        }
      }
    }
  }
}"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ES2022",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  }
}"#,
            },
            EmbeddedFile {
                path: "tsconfig.app.json",
                content: r#"{
  "extends": "./tsconfig.json",
  "compilerOptions": {
    "noEmit": true,
    "types": [],
    "isolatedModules": true
  },
  "include": ["src/**/*.ts", "src/**/*.d.ts"]
}"#,
            },
            EmbeddedFile {
                path: "src/index.html",
                content: r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>{{PROJECT_NAME}}</title>
    <base href="/" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
  </head>
  <body>
    <app-root></app-root>
  </body>
</html>
"#,
            },
            EmbeddedFile {
                path: "src/styles.css",
                content: r#"body { font-family: sans-serif; padding: 2rem; }
"#,
            },
            EmbeddedFile {
                path: "src/main.ts",
                content: r#"import { bootstrapApplication } from '@angular/platform-browser';
import { AppComponent } from './app/app.component';

bootstrapApplication(AppComponent).catch((err: unknown) => console.error(err));
"#,
            },
            EmbeddedFile {
                path: "src/app/app.component.ts",
                content: r#"import { Component } from '@angular/core';

@Component({
  selector: 'app-root',
  standalone: true,
  template: `<main><h1>Welcome to {{PROJECT_NAME}}</h1><p>Powered by MagiCore & Angular</p></main>`,
})
export class AppComponent {}
"#,
            },
        ]),
        ("web", "solidjs") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite --port 4315",
    "build": "vite build",
    "preview": "vite preview --port 4315",
    "test": "tsc --noEmit"
  },
  "dependencies": {
    "solid-js": "^1.9.0"
  },
  "devDependencies": {
    "typescript": "^5.7.2",
    "vite": "^6.0.0",
    "vite-plugin-solid": "^2.11.0"
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
    <script type="module" src="/src/index.tsx"></script>
  </body>
</html>"#,
            },
            EmbeddedFile {
                path: "src/index.tsx",
                content: r#"import { render } from 'solid-js/web';
import App from './App';

render(() => <App />, document.getElementById('root')!);
"#,
            },
            EmbeddedFile {
                path: "src/App.tsx",
                content: r#"export default function App() {
  return (
    <main style={{ padding: '2rem', 'font-family': 'sans-serif' }}>
      <h1>Welcome to {{PROJECT_NAME}}</h1>
      <p>Powered by MagiCore & SolidJS</p>
    </main>
  );
}
"#,
            },
            EmbeddedFile {
                path: "vite.config.ts",
                content: r#"import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

export default defineConfig({ plugins: [solid()] });
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "preserve",
    "jsxImportSource": "solid-js",
    "strict": true
  },
  "include": ["src"]
}"#,
            },
        ]),
        ("web", "qwik") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite --mode ssr --port 4315",
    "build": "vite build",
    "preview": "vite preview --port 4315"
  },
  "dependencies": {
    "@builder.io/qwik": "^1.12.0"
  },
  "devDependencies": {
    "typescript": "^5.7.2",
    "vite": "^6.0.0"
  }
}"#,
            },
            EmbeddedFile {
                path: "vite.config.ts",
                content: r#"import { defineConfig } from 'vite';
import { qwikVite } from '@builder.io/qwik/optimizer';

export default defineConfig({
  plugins: [qwikVite()],
  server: { port: 4315 },
});
"#,
            },
            EmbeddedFile {
                path: "src/root.tsx",
                content: r#"import { component$ } from '@builder.io/qwik';

export default component$(() => {
  return (
    <main style={{ padding: '2rem', fontFamily: 'sans-serif' }}>
      <h1>Welcome to {{PROJECT_NAME}}</h1>
      <p>Powered by MagiCore & Qwik</p>
    </main>
  );
});
"#,
            },
            EmbeddedFile {
                path: "src/entry.dev.tsx",
                content: r#"import { render, type RenderOptions } from '@builder.io/qwik';
import Root from './root';

export default function (opts: RenderOptions) {
  return render(document, <Root />, opts);
}
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "jsxImportSource": "@builder.io/qwik",
    "strict": true
  },
  "include": ["src"]
}"#,
            },
        ]),
        ("web", "astro") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "astro dev --port 4315",
    "build": "astro build",
    "preview": "astro preview --port 4315"
  },
  "dependencies": {
    "astro": "^5.0.0"
  },
  "devDependencies": {
    "typescript": "^5.7.2"
  }
}"#,
            },
            EmbeddedFile {
                path: "astro.config.mjs",
                content: r#"import { defineConfig } from 'astro/config';

export default defineConfig({});
"#,
            },
            EmbeddedFile {
                path: "src/pages/index.astro",
                content: r#"---
const project = "{{PROJECT_NAME}}";
---
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{project}</title>
  </head>
  <body>
    <main style="padding: 2rem; font-family: sans-serif">
      <h1>Welcome to {project}</h1>
      <p>Powered by MagiCore & Astro</p>
    </main>
  </body>
</html>
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "extends": "astro/tsconfigs/strict",
  "include": ["src"]
}"#,
            },
        ]),
        ("web", "remix") => Some(vec![
            EmbeddedFile {
                path: "package.json",
                content: r#"{
  "name": "{{PROJECT_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "remix dev --port 4315",
    "build": "remix vite:build",
    "start": "remix-serve ./build/server/index.js"
  },
  "dependencies": {
    "@remix-run/node": "^2.17.0",
    "@remix-run/react": "^2.17.0",
    "@remix-run/serve": "^2.17.0",
    "isbot": "^4.4.0",
    "react": "^18.3.0",
    "react-dom": "^18.3.0"
  },
  "devDependencies": {
    "@remix-run/dev": "^2.17.0",
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "typescript": "^5.7.2",
    "vite": "^6.0.0"
  }
}"#,
            },
            EmbeddedFile {
                path: "vite.config.ts",
                content: r#"import { vitePlugin as remix } from "@remix-run/dev";
import { defineConfig } from "vite";

export default defineConfig({ plugins: [remix()] });
"#,
            },
            EmbeddedFile {
                path: "app/root.tsx",
                content: r#"import { Links, Meta, Outlet, Scripts, ScrollRestoration } from "@remix-run/react";

export default function App() {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <Meta />
        <Links />
      </head>
      <body>
        <Outlet />
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}
"#,
            },
            EmbeddedFile {
                path: "app/routes/_index.tsx",
                content: r#"export default function Index() {
  return (
    <main style={{ padding: "2rem", fontFamily: "sans-serif" }}>
      <h1>Welcome to {{PROJECT_NAME}}</h1>
      <p>Powered by MagiCore & Remix</p>
    </main>
  );
}
"#,
            },
            EmbeddedFile {
                path: "tsconfig.json",
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true
  },
  "include": ["app", ".remix"]
}"#,
            },
        ]),
        ("web", "actix-web") => Some(vec![
            EmbeddedFile {
                path: "Cargo.toml",
                content: r#"[package]
name = "{{PROJECT_NAME}}"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web = "4"
"#,
            },
            EmbeddedFile {
                path: "src/main.rs",
                content: r#"use actix_web::{App, HttpResponse, HttpServer, get};

#[get("/")]
async fn index() -> HttpResponse {
    HttpResponse::Ok().body("Welcome to {{PROJECT_NAME}} (MagiCore + actix-web)")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(index))
        .bind(("127.0.0.1", 4315))?
        .run()
        .await
}
"#,
            },
        ]),
        ("web", "gin") => Some(vec![
            EmbeddedFile {
                path: "go.mod",
                content: r#"module {{PROJECT_NAME}}

go 1.21
"#,
            },
            EmbeddedFile {
                path: "main.go",
                content: r#"package main

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

func main() {
	r := gin.Default()
	r.GET("/", func(c *gin.Context) {
		c.String(http.StatusOK, "Welcome to {{PROJECT_NAME}} (MagiCore + Gin)")
	})
	_ = r.Run("127.0.0.1:4315")
}
"#,
            },
        ]),
        ("web", "echo") => Some(vec![
            EmbeddedFile {
                path: "go.mod",
                content: r#"module {{PROJECT_NAME}}

go 1.21
"#,
            },
            EmbeddedFile {
                path: "main.go",
                content: r#"package main

import (
	"net/http"

	"github.com/labstack/echo/v4"
)

func main() {
	e := echo.New()
	e.GET("/", func(c echo.Context) error {
		return c.String(http.StatusOK, "Welcome to {{PROJECT_NAME}} (MagiCore + Echo)")
	})
	e.Logger.Fatal(e.Start("127.0.0.1:4315"))
}
"#,
            },
        ]),
        ("web", "fiber") => Some(vec![
            EmbeddedFile {
                path: "go.mod",
                content: r#"module {{PROJECT_NAME}}

go 1.21
"#,
            },
            EmbeddedFile {
                path: "main.go",
                content: r#"package main

import (
	"log"

	"github.com/gofiber/fiber/v2"
)

func main() {
	app := fiber.New()
	app.Get("/", func(c *fiber.Ctx) error {
		return c.SendString("Welcome to {{PROJECT_NAME}} (MagiCore + Fiber)")
	})
	log.Fatal(app.Listen("127.0.0.1:4315"))
}
"#,
            },
        ]),
        ("web", "django") => Some(vec![
            EmbeddedFile {
                path: "pyproject.toml",
                content: r#"[project]
name = "{{PROJECT_NAME}}"
version = "0.1.0"
dependencies = []
"#,
            },
            EmbeddedFile {
                path: "app.py",
                content: r#"# Minimal Django check (scaffold coherence, no server).
import django
from django.conf import settings

settings.configure(
    DEBUG=False,
    ROOT_URLCONF=__name__,
    SECRET_KEY="magicore-scaffold",
    ALLOWED_HOSTS=["*"],
)

print("django", django.get_version(), "ok for {{PROJECT_NAME}}")
"#,
            },
        ]),
        ("web", "flask") => Some(vec![
            EmbeddedFile {
                path: "pyproject.toml",
                content: r#"[project]
name = "{{PROJECT_NAME}}"
version = "0.1.0"
dependencies = []
"#,
            },
            EmbeddedFile {
                path: "app.py",
                content: r#"from flask import Flask

app = Flask(__name__)


@app.get("/")
def index():
    return "Welcome to {{PROJECT_NAME}} (MagiCore + Flask)"


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=4315)
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
