#!/usr/bin/env python3
"""Fill missing source files for react-* and vue-* split fullstack templates."""
import os

BASE = "/Users/doanmihh/Documents/Workspace/MegaGate/templates/web/fullstack/split"

def ensure_dir(path):
    os.makedirs(path, exist_ok=True)

def write_file(path, content):
    ensure_dir(os.path.dirname(path))
    with open(path, "w") as f:
        f.write(content.lstrip("\n"))

def read_file(path):
    with open(path) as f:
        return f.read()

# Common React client sources
REACT_CLIENT = {
    "client/index.ts.html": """<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/><title>{{ project_name }}</title></head><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>
""",
    "client/index.js.html": """<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/><title>{{ project_name }}</title></head><body><div id="root"></div><script type="module" src="/src/main.jsx"></script></body></html>
""",
    "client/tsconfig.json": """{ "compilerOptions": { "target": "ES2020", "useDefineForClassFields": true, "lib": ["ES2020", "DOM", "DOM.Iterable"], "module": "ESNext", "skipLibCheck": true, "moduleResolution": "bundler", "allowImportingTsExtensions": true, "resolveJsonModule": true, "isolatedModules": true, "noEmit": true, "jsx": "react-jsx", "strict": true }, "include": ["src"] }
""",
    "client/jsconfig.json": """{ "compilerOptions": { "target": "ES2020", "module": "ESNext", "moduleResolution": "bundler", "jsx": "react-jsx" } }
""",
    "client/vite-env.d.ts": """/// <reference types="vite/client" />
""",
    "client/vite.config.ts": """import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({ plugins: [react()], server: { port: 5173 } });
""",
    "client/vite.config.js": """import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({ plugins: [react()], server: { port: 5173 } });
""",
    "client/main.tsx": """import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.tsx";
createRoot(document.getElementById("root")!).render(<StrictMode><App /></StrictMode>);
""",
    "client/main.jsx": """import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.jsx";
createRoot(document.getElementById("root")!).render(<StrictMode><App /></StrictMode>);
""",
    "client/App.tsx": """export default function App() { return <main><h1>{{ project_name }}</h1></main>; }
""",
    "client/App.jsx": """export default function App() { return <main><h1>{{ project_name }}</h1></main>; }
""",
}

# Common server sources (shared between express/fastify/hono)
NODE_SERVER = {
    "server/package.ts.json": """{ "name": "@project/server", "private": true, "type": "module", "version": "0.1.0",
  "scripts": { "dev": "tsx watch src/server.ts", "build": "tsc", "start": "node dist/server.js" } }
""",
    "server/package.js.json": """{ "name": "@project/server", "private": true, "type": "module", "version": "0.1.0",
  "scripts": { "dev": "node --watch src/server.js", "start": "node src/server.js" } }
""",
    "server/tsconfig.json": """{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "strict": true, "esModuleInterop": true, "skipLibCheck": true, "outDir": "dist" }, "include": ["src"] }
""",
    "server/server.ts": """import { app } from "./lib/app.js";
const port = parseInt(process.env.PORT || "3000", 10);
app.listen(port, () => console.log(`running on :${port}`));
""",
    "server/server.js": """import { app } from "./lib/app.js";
const port = parseInt(process.env.PORT || "3000", 10);
app.listen(port, () => console.log(`running on :${port}`));
""",
}

# Fill react-express and react-hono
for t in ["react-express", "react-hono"]:
    d = f"{BASE}/{t}"
    if not os.path.exists(f"{d}/sources/client") or not os.listdir(f"{d}/sources/client"):
        print(f"  Filling {t}...")
        for name, content in {**REACT_CLIENT, **NODE_SERVER}.items():
            write_file(f"{d}/sources/{name}", content)

# Also fill node server common files for react-nestjs and react-trpc
for t in ["react-nestjs", "react-trpc"]:
    d = f"{BASE}/{t}"
    needs_client = not os.path.exists(f"{d}/sources/client") or not os.listdir(f"{d}/sources/client")
    if needs_client:
        print(f"  Filling {t} client...")
        for name, content in REACT_CLIENT.items():
            write_file(f"{d}/sources/{name}", content)

print("Done")
