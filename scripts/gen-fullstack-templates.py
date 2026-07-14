#!/usr/bin/env python3
"""Generate monorepo backend + fullstack templates."""
import os

BASE = "/Users/doanmihh/Documents/Workspace/MegaGate/templates/web"

def ensure_dir(path):
    os.makedirs(path, exist_ok=True)

def write_file(path, content):
    ensure_dir(os.path.dirname(path))
    with open(path, "w") as f:
        f.write(content.lstrip("\n"))

def read_file(path):
    with open(path) as f:
        return f.read()

# === 1. Monorepo backend templates ===
# Copy standalone backend template.toml but prefix targets with apps/backend/
# And copy source files

def make_monorepo_toml(standalone_toml):
    """Convert standalone template.toml to monorepo (prefix targets)."""
    lines = []
    for line in standalone_toml.split("\n"):
        if line.strip().startswith('target = "') and not line.strip().startswith('target = "apps/'):
            path = line.split('target = "')[1].rstrip('"')
            line = line.replace(f'target = "{path}"', f'target = "apps/backend/{path}"')
        lines.append(line)
    return "\n".join(lines)

# Copy standalone backends to monorepo
for fw in ["express", "fastify", "nestjs", "hono", "trpc"]:
    src = f"{BASE}/backend/node/{fw}"
    dst = f"{BASE}/monorepo/backend/node/{fw}"
    ensure_dir(f"{dst}/sources")
    
    # Copy template.toml with modified targets
    standalone_toml = read_file(f"{src}/template.toml")
    monorepo_toml = make_monorepo_toml(standalone_toml)
    # Remove docker blocks from monorepo (handled by monorepo partial)
    blocks = monorepo_toml.split("\n\n")
    filtered = []
    for block in blocks:
        if 'source = "Dockerfile"' in block or 'source = ".dockerignore"' in block:
            continue
        filtered.append(block)
    monorepo_toml = "\n\n".join(filtered)
    write_file(f"{dst}/template.toml", monorepo_toml)
    
    # Copy source files
    for f in os.listdir(f"{src}/sources"):
        full = f"{src}/sources/{f}"
        if os.path.isfile(full) and f not in ["Dockerfile", ".dockerignore"]:
            write_file(f"{dst}/sources/{f}", read_file(full))

# Non-Node backends
for lang, fws in [("go", ["gin", "echo", "fiber"]), ("python", ["fastapi", "flask", "django"]),
                   ("java", ["spring-boot", "quarkus"]), ("php", ["laravel", "symfony"]),
                   ("rust", ["axum", "actix-web"])]:
    for fw in fws:
        src = f"{BASE}/backend/{lang}/{fw}"
        dst = f"{BASE}/monorepo/backend/{lang}/{fw}"
        ensure_dir(f"{dst}/sources")
        
        standalone_toml = read_file(f"{src}/template.toml")
        monorepo_toml = make_monorepo_toml(standalone_toml)
        write_file(f"{dst}/template.toml", monorepo_toml)
        
        for f in os.listdir(f"{src}/sources"):
            full = f"{src}/sources/{f}"
            if os.path.isfile(full):
                write_file(f"{dst}/sources/{f}", read_file(full))

print("--- Monorepo backend templates done ---")

# === 2. Fullstack all-in-one ===
# Create all-in-one templates for nextjs, nuxt, sveltekit (remix already exists)

ALLO_SOURCES = {
    "nextjs": {
        "package.json": """{
  "name": "@project/app",
  "private": true,
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start"
  }
}
""",
        "tsconfig.json": """{ "compilerOptions": { "target": "es5", "lib": ["dom", "dom.iterable", "esnext"], "allowJs": true, "skipLibCheck": true, "strict": true, "noEmit": true, "esModuleInterop": true, "module": "esnext", "moduleResolution": "bundler", "resolveJsonModule": true, "isolatedModules": true, "jsx": "preserve", "incremental": true, "plugins": [{ "name": "next" }] }, "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"], "exclude": ["node_modules"] }
""",
        "app/layout.tsx": """export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (<html lang="en"><body>{children}</body></html>);
}
""",
        "app/page.tsx": """export default function Home() { return (<main><h1>Welcome</h1></main>); }
""",
        "app/api/health/route.ts": """export async function GET() { return Response.json({ status: "ok" }); }
""",
    },
    "nuxt": {
        "package.json": """{ "name": "@project/app", "private": true, "scripts": { "dev": "nuxt dev", "build": "nuxt build", "start": "node .output/server/index.mjs" } }
""",
        "tsconfig.json": """{ "extends": "./.nuxt/tsconfig.json" }
""",
        "nuxt.config.ts": """export default defineNuxtConfig({ devtools: { enabled: true } });
""",
        "app.vue": """<template><main><h1>Welcome</h1></main></template>
""",
        "server/api/health.ts": """export default defineEventHandler(() => ({ status: "ok" }));
""",
    },
    "sveltekit": {
        "package.json": """{ "name": "@project/app", "private": true, "type": "module", "scripts": { "dev": "vite dev", "build": "vite build", "preview": "vite preview" } }
""",
        "svelte.config.js": """import adapter from '@sveltejs/adapter-auto';
export default { kit: { adapter: adapter() } };
""",
        "vite.config.ts": """import { sveltekit } from '@sveltejs/kit/vite';
export default { plugins: [sveltekit()] };
""",
        "src/app.html": """<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"/><title>Welcome</title></head><body><div id="svelte">%sveltekit.body%</div></body></html>
""",
        "src/routes/+page.svelte": """<main><h1>Welcome</h1></main>
""",
        "src/routes/api/health/+server.ts": """export const GET = () => Response.json({ status: "ok" });
""",
    },
}

for fw, sources in ALLO_SOURCES.items():
    d = f"{BASE}/fullstack/all-in-one/{fw}"
    ensure_dir(f"{d}/sources")
    for name, content in sources.items():
        write_file(f"{d}/sources/{name}", content)

print("--- All-in-one templates done ---")

# === 3. Fullstack split templates ===
# Generate for all frontend + node backend combos
# Non-node backends: add basic workspace template

FRONTENDS = ["react-vite", "vue-vite", "sveltekit", "solidjs", "astro", "qwik", "vanilla"]
NODE_BACKENDS = ["express", "fastify", "hono", "nestjs"]

SPLIT_ROOT = {
    "package.json": """{
  "name": "@project/app",
  "private": true,
  "workspaces": ["client", "server"]
}
""",
}

SPLIT_CLIENT = {
    "package.json": """{ "name": "@project/client", "private": true, "version": "0.1.0" }
""",
}

SPLIT_SERVER = {
    "package.ts.json": """{ "name": "@project/server", "private": true, "type": "module", "version": "0.1.0",
  "scripts": { "dev": "tsx watch src/server.ts", "build": "tsc", "start": "node dist/server.js" } }
""",
    "package.js.json": """{ "name": "@project/server", "private": true, "type": "module", "version": "0.1.0",
  "scripts": { "dev": "node --watch src/server.js", "start": "node src/server.js" } }
""",
    "tsconfig.json": """{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "strict": true, "esModuleInterop": true, "skipLibCheck": true, "outDir": "dist" }, "include": ["src"] }
""",
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
}

SPLIT_FE = {
    "react-vite": """import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
createRoot(document.getElementById("root")!).render(<StrictMode><h1>Welcome</h1></StrictMode>);
""",
    "vue-vite": """<template><main><h1>Welcome</h1></main></template>
<script setup></script>
""",
    "solidjs": """import { render } from "solid-js/web";
render(() => <main><h1>Welcome</h1></main>, document.getElementById("root")!);
""",
    "vanilla": """document.querySelector("#app")!.innerHTML = "<h1>Welcome</h1>";
""",
}

def gen_split_toml(fe, be):
    """Generate template.toml for a split fullstack combo."""
    return f"""[[files]]
source = "root/package.json"
target = "package.json"
required_context = ["project_slug"]

[[files]]
source = "client/package.json"
target = "client/package.json"
required_context = ["project_slug"]

[[files]]
source = "server/package.ts.json"
target = "server/package.json"
required_context = ["project_slug"]
include_features = ["typescript"]

[[files]]
source = "server/package.js.json"
target = "server/package.json"
required_context = ["project_slug"]
exclude_features = ["typescript"]

[[files]]
source = "server/tsconfig.json"
target = "server/tsconfig.json"
include_features = ["typescript"]

[[files]]
source = "server/server.ts"
target = "server/src/server.ts"
include_features = ["typescript"]

[[files]]
source = "server/server.js"
target = "server/src/server.js"
exclude_features = ["typescript"]

[[files]]
source = "server/app.ts"
target = "server/src/app.ts"
include_features = ["typescript"]

[[files]]
source = "server/app.js"
target = "server/src/app.js"
exclude_features = ["typescript"]

[[files]]
source = "client/app.tsx"
target = "client/src/App.tsx"
include_features = ["typescript"]
required_context = ["project_name"]

[[files]]
source = "client/app.jsx"
target = "client/src/App.jsx"
exclude_features = ["typescript"]
required_context = ["project_name"]

[[files]]
source = "client/vite.config.ts"
target = "client/vite.config.ts"
include_features = ["typescript"]

[[files]]
source = "client/vite.config.js"
target = "client/vite.config.js"
exclude_features = ["typescript"]
"""

for fe in FRONTENDS:
    for be in NODE_BACKENDS:
        d = f"{BASE}/fullstack/split/{fe}-{be}"
        ensure_dir(f"{d}/sources")
        ensure_dir(f"{d}/sources/root")
        ensure_dir(f"{d}/sources/client")
        ensure_dir(f"{d}/sources/server")
        
        write_file(f"{d}/sources/root/package.json", SPLIT_ROOT["package.json"])
        write_file(f"{d}/sources/client/package.json", SPLIT_CLIENT["package.json"])
        for name, content in SPLIT_SERVER.items():
            write_file(f"{d}/sources/server/{name}", content)
        
        # Minimal FE source
        fe_code = SPLIT_FE.get(fe, "")
        write_file(f"{d}/sources/client/app.tsx", fe_code)
        write_file(f"{d}/sources/client/app.jsx", fe_code)
        write_file(f"{d}/sources/client/vite.config.ts", """import { defineConfig } from "vite";
export default defineConfig({ server: { port: 5173 } });
""")
        write_file(f"{d}/sources/client/vite.config.js", """import { defineConfig } from "vite";
export default defineConfig({ server: { port: 5173 } });
""")
        
        write_file(f"{d}/template.toml", gen_split_toml(fe, be))

print("--- Split fullstack templates done ---")
print("All templates generated!")
