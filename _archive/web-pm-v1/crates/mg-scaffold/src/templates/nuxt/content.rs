use crate::versions::*;

pub struct Ctx {
    pub name: String,
    pub version: String,
}

impl Ctx {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self { name: name.into(), version: version.into() }
    }
}

pub fn package_json(ctx: &Ctx) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "version": "{version}",
  "private": true,
  "type": "module",
  "scripts": {{
    "dev": "nuxt dev",
    "build": "nuxt build",
    "generate": "nuxt generate",
    "preview": "nuxt preview",
    "lint": "eslint .",
    "format": "prettier --write ."
  }},
  "dependencies": {{
    "nuxt": "{NUXT}",
    "vue": "{VUE}",
    "vue-router": "{VUE_ROUTER}",
    "pinia": "{PINIA}"
  }},
  "devDependencies": {{
    "@nuxt/eslint": "{NUXT_ESLINT}",
    "typescript": "{TYPESCRIPT}",
    "prettier": "{PRETTIER}",
    "@types/node": "{TYPES_NODE}"
  }}
}}"#,
        name = ctx.name,
        version = ctx.version,
        NUXT = NUXT(),
        VUE = VUE(),
        VUE_ROUTER = VUE_ROUTER(),
        PINIA = PINIA(),
        NUXT_ESLINT = NUXT_ESLINT(),
        TYPESCRIPT = TYPESCRIPT(),
        PRETTIER = PRETTIER(),
        TYPES_NODE = TYPES_NODE(),
    )
}

pub fn nuxt_config_ts() -> &'static str {
    r#"export default defineNuxtConfig({
  devtools: { enabled: true },
  modules: ['@nuxt/eslint'],
  css: ['~/assets/css/main.css'],
  compatibilityDate: '2025-12-01',
});"#
}

pub fn tsconfig_json() -> &'static str {
    r#"{
  "extends": "./.nuxt/tsconfig.json",
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "verbatimModuleSyntax": true,
    "skipLibCheck": true
  }
}"#
}

pub fn app_vue() -> &'static str {
    r#"<template>
  <div>
    <NuxtRouteAnnouncer />
    <NuxtPage />
  </div>
</template>"#
}

pub fn index_vue(ctx: &Ctx) -> String {
    format!(
        r#"<script setup lang="ts">
</script>

<template>
  <div class="home">
    <h1>Welcome to {name}</h1>
    <p>Built with Nuxt + TypeScript</p>
    <NuxtLink to="/about">Learn more</NuxtLink>
  </div>
</template>

<style scoped>
.home {{
  text-align: center;
  padding: 4rem 2rem;
}}
h1 {{ font-size: 2.5rem; margin-bottom: 1rem; }}
p {{ font-size: 1.1rem; color: #666; margin-bottom: 1.5rem; }}
</style>"#,
        name = ctx.name,
    )
}

pub fn about_vue(ctx: &Ctx) -> String {
    format!(
        r#"<template>
  <div class="about">
    <h1>About {name}</h1>
    <p>A modern web app built with Nuxt.</p>
    <NuxtLink to="/">Go home</NuxtLink>
  </div>
</template>"#,
        name = ctx.name,
    )
}

pub fn header_vue() -> &'static str {
    r#"<template>
  <header>
    <nav>
      <NuxtLink to="/">Home</NuxtLink>
      <NuxtLink to="/about">About</NuxtLink>
    </nav>
  </header>
</template>

<style scoped>
header {
  padding: 1rem 2rem;
  border-bottom: 1px solid #e5e7eb;
}
nav { display: flex; gap: 1rem; }
a { color: #3b82f6; text-decoration: none; font-weight: 500; }
a:hover { text-decoration: underline; }
</style>"#
}

pub fn use_auth_ts() -> &'static str {
    r#"import { useAuthStore } from '~/stores/auth';

export function useAuth() {
  const authStore = useAuthStore();

  const signIn = async (email: string, password: string) => {
    await authStore.signIn(email, password);
  };

  const signOut = () => {
    authStore.signOut();
  };

  return {
    user: computed(() => authStore.user),
    isAuthenticated: computed(() => authStore.isAuthenticated),
    signIn,
    signOut,
  };
}"#
}

pub fn auth_store_ts() -> &'static str {
    r#"import { ref } from 'vue';
import { defineStore } from 'pinia';
import type { User } from '~/types';

export const useAuthStore = defineStore('auth', () => {
  const user = ref<User | null>(null);
  const isAuthenticated = ref(false);

  async function signIn(email: string, _password: string) {
    user.value = { id: '1', email };
    isAuthenticated.value = true;
  }

  function signOut() {
    user.value = null;
    isAuthenticated.value = false;
  }

  return { user, isAuthenticated, signIn, signOut };
});"#
}

pub fn api_ts() -> &'static str {
    r#"const API_BASE = import.meta.env.NUXT_PUBLIC_API_URL || 'http://localhost:3001';

class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = 'ApiError';
  }
}

function sanitizeError(status: number, body: unknown): string {
  const safe = [400, 401, 403, 404, 422];
  if (safe.includes(status) && typeof body === 'object' && body && 'message' in body) {
    return String(body.message);
  }
  return 'An unexpected error occurred';
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });

  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new ApiError(res.status, sanitizeError(res.status, body));
  }

  return res.json();
}"#
}

pub fn types_index_ts() -> &'static str {
    r#"export interface User {
  id: string;
  email: string;
  name?: string;
}

export interface ApiResponse<T> {
  data: T;
  message?: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}"#
}

pub fn helpers_ts() -> &'static str {
    r#"export function cn(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(' ');
}

export function formatDate(date: Date): string {
  return new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  }).format(date);
}"#
}

pub fn main_css() -> &'static str {
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
}"#
}

pub fn gitignore() -> &'static str {
    r#"node_modules/
.nuxt/
.output/
dist/
.env
.env.local
*.log
.DS_Store"#
}

pub fn env_example() -> &'static str {
    r#"# Nuxt
NUXT_PUBLIC_API_URL=http://localhost:3001"#
}

pub fn readme(ctx: &Ctx) -> String {
    format!(
        r#"# {name}

Built with Nuxt + TypeScript.

## Commands

```bash
npm run dev       # Start dev server
npm run build     # Build for production
npm run preview   # Preview production build
```

## Structure

```
pages/          # File-based routing
components/     # Reusable components
composables/    # Vue composables
stores/         # Pinia stores
services/       # API services
types/          # TypeScript types
utils/          # Utility functions
assets/         # Static assets (CSS, images)
```
"#,
        name = ctx.name,
    )
}
