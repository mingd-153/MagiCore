use heck::ToUpperCamelCase;
use crate::versions::*;

pub struct Ctx {
    pub name: String,
    pub version: String,
}

impl Ctx {
    pub fn new(name: &str, version: &str) -> Self {
        Self { name: name.to_string(), version: version.to_string() }
    }
}

pub fn package_json(ctx: &Ctx) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "version": "{version}",
  "type": "module",
  "scripts": {{
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview",
    "lint": "eslint .",
    "format": "prettier --write ."
  }},
  "dependencies": {{
    "astro": "{astro}"
  }},
  "devDependencies": {{
    "@astrojs/check": "{astro_check}",
    "typescript": "{typescript}"
  }}
}}"#,
        name = ctx.name, version = ctx.version,
        astro = ASTRO(), astro_check = ASTRO_CHECK(),
        typescript = TYPESCRIPT(),
    )
}

pub fn tsconfig_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "jsx": "preserve",
    "verbatimModuleSyntax": true,
    "skipLibCheck": true,
    "types": ["astro/client"]
  },
  "include": ["src/**/*.ts", "src/**/*.astro"]
}"#
    .into()
}

pub fn astro_config_mjs() -> String {
    r#"import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://example.com',
  output: 'static',
});
"#
    .into()
}

pub fn env_dts() -> String {
    r#"/// <reference types="astro/client" />
"#
    .into()
}

pub fn index_astro(ctx: &Ctx) -> String {
    format!(
        r#"---
import Layout from '../layouts/Layout.astro';
---

<Layout title="{name}">
  <main>
    <h1>Welcome to {name}</h1>
    <p>Built with Astro + TypeScript</p>
    <a href="/about">Learn more</a>
  </main>
</Layout>

<style>
  main {{
    text-align: center;
    padding: 4rem 2rem;
  }}
  h1 {{ font-size: 2.5rem; margin-bottom: 1rem; }}
  p {{ font-size: 1.1rem; color: #666; margin-bottom: 1.5rem; }}
</style>
"#,
        name = ctx.name,
    )
}

pub fn about_astro(ctx: &Ctx) -> String {
    format!(
        r#"---
import Layout from '../layouts/Layout.astro';
---

<Layout title="About - {name}">
  <main>
    <h1>About {name}</h1>
    <p>A modern static site built with Astro.</p>
    <a href="/">Go home</a>
  </main>
</Layout>
"#,
        name = ctx.name,
    )
}

pub fn layout_astro() -> String {
    r#"---
export interface Props {
  title: string;
}

const { title } = Astro.props;
---

<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <link rel="icon" href="/favicon.ico" sizes="32x32" />
    <title>{title}</title>
  </head>
  <body>
    <slot />
  </body>
</html>
"#
    .into()
}

pub fn header_astro() -> String {
    r#"---
---

<header>
  <nav>
    <a href="/">Home</a>
    <a href="/about">About</a>
  </nav>
</header>

<style>
  header {
    padding: 1rem 2rem;
    border-bottom: 1px solid #e5e7eb;
  }
  nav { display: flex; gap: 1rem; }
  a { color: #3b82f6; text-decoration: none; font-weight: 500; }
  a:hover { text-decoration: underline; }
</style>
"#
    .into()
}

pub fn global_css() -> String {
    r#"*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
}

html { font-family: system-ui, sans-serif; }

body {
  min-height: 100vh;
  color: #111;
  background: #fff;
}
"#
    .into()
}

pub fn gitignore() -> String {
    r#"node_modules/
dist/
.env
.env.local
*.log
.DS_Store
"#
    .into()
}

pub fn env_example() -> String {
    r#"# Astro
PUBLIC_SITE_URL=http://localhost:4321
"#
    .into()
}

pub fn readme(ctx: &Ctx) -> String {
    let pascal = ctx.name.to_upper_camel_case();
    format!(
        r#"# {pascal}

Built with Astro + TypeScript.

## Commands

```bash
mg dev       # Start dev server
mg build     # Build for production
mg preview   # Preview production build
```

## Structure

```
src/
├── pages/        # Routes
├── layouts/      # Page layouts
├── components/   # Reusable components
└── styles/       # Global styles
```
"#,
        pascal = pascal,
    )
}
