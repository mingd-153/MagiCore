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
  "private": true,
  "version": "{version}",
  "type": "module",
  "scripts": {{
    "dev": "vite",
    "build": "vue-tsc -b && vite build",
    "preview": "vite preview",
    "lint": "eslint .",
    "format": "prettier --write ."
  }},
  "dependencies": {{
    "vue": "{vue}",
    "vue-router": "{vue_router}",
    "pinia": "{pinia}"
  }},
  "devDependencies": {{
    "@vitejs/plugin-vue": "{vite_plugin_vue}",
    "typescript": "{typescript}",
    "vite": "{vite}",
    "vue-tsc": "{vue_tsc}",
    "eslint": "{eslint}",
    "@eslint/js": "{eslint_js}",
    "typescript-eslint": "{typescript_eslint}",
    "eslint-plugin-vue": "{eslint_plugin_vue}",
    "vue-eslint-parser": "{vue_eslint_parser}",
    "globals": "{globals}",
    "@types/node": "{types_node}",
    "prettier": "{prettier}"
  }}
}}"#,
        name = ctx.name, version = ctx.version,
        vue = VUE(), vue_router = VUE_ROUTER(), pinia = PINIA(),
        vite_plugin_vue = VITE_PLUGIN_VUE(), typescript = TYPESCRIPT(),
        vite = VITE(), vue_tsc = VUE_TSC(), eslint = ESLINT(),
        eslint_js = ESLINT_JS(), typescript_eslint = TYPESCRIPT_ESLINT(),
        eslint_plugin_vue = ESLINT_PLUGIN_VUE(),
        vue_eslint_parser = VUE_ESLINT_PARSER(), globals = GLOBALS(),
        types_node = TYPES_NODE(), prettier = PRETTIER(),
    )
}

pub fn index_html(ctx: &Ctx) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<link rel="icon" href="/favicon.ico" sizes="32x32" />
<title>{name}</title>
</head>
<body>
<div id="app"></div>
<script type="module" src="/src/main.ts"></script>
</body>
</html>
"#,
        name = ctx.name,
    )
}

pub fn tsconfig_json() -> String {
    r#"{
  "files": [],
  "references": [
    { "path": "./tsconfig.app.json" },
    { "path": "./tsconfig.node.json" }
  ]
}"#
    .into()
}

pub fn tsconfig_app_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "preserve",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "noUncheckedIndexedAccess": true,
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src/**/*.ts", "src/**/*.tsx", "src/**/*.vue", "env.d.ts"]
}"#
    .into()
}

pub fn tsconfig_node_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2023"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["vite.config.ts"]
}"#
    .into()
}

pub fn vite_config_ts() -> String {
    r#"import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, './src'),
    },
  },
  server: {
    port: 4315,
    open: true,
  },
  preview: {
    port: 4316,
  },
  build: {
    target: 'es2022',
  },
});
"#
    .into()
}

pub fn eslint_config() -> String {
    r#"import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import vueParser from 'vue-eslint-parser';
import vue from 'eslint-plugin-vue';
import globals from 'globals';

export default tseslint.config(
  { ignores: ['dist'] },
  {
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      ...vue.configs['flat/recommended'],
    ],
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
      parser: vueParser,
      parserOptions: {
        ecmaFeatures: { jsx: true },
        parser: tseslint.parser,
      },
    },
    plugins: {
      vue,
    },
    rules: {
      'vue/multi-word-component-names': 'off',
      'vue/no-unused-vars': 'warn',
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
    },
  },
);
"#
    .into()
}

pub fn env_dts() -> String {
    r#"/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_URL: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<object, object, unknown>;
  export default component;
}
"#
    .into()
}

pub fn main_ts() -> String {
    r#"import { createApp } from 'vue';
import { createPinia } from 'pinia';
import router from '@/router';
import App from '@/App.vue';
import '@/styles/main.css';

const app = createApp(App);

app.use(createPinia());
app.use(router);

app.mount('#app');
"#
    .into()
}

pub fn app_vue() -> String {
    r#"<script setup lang="ts">
import { RouterView } from 'vue-router';
import AppHeader from '@/components/features/AppHeader.vue';
</script>

<template>
  <div class="app">
    <AppHeader />
    <main>
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.app {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

main {
  flex: 1;
  max-width: 960px;
  width: 100%;
  margin: 0 auto;
  padding: 32px var(--spacing);
}
</style>
"#
    .into()
}

pub fn home_page_vue(ctx: &Ctx) -> String {
    format!(
        r#"<script setup lang="ts">
import {{ RouterLink }} from 'vue-router';
</script>

<template>
  <div class="home">
    <h1>Welcome to {name}</h1>
    <p>Built with Vue 3 + TypeScript + Vite</p>
    <RouterLink to="/about">Learn more</RouterLink>
  </div>
</template>

<style scoped>
.home {{
  text-align: center;
  padding: 4rem 2rem;
}}

h1 {{
  font-size: 2.5rem;
  margin-bottom: 1rem;
}}

p {{
  font-size: 1.1rem;
  color: #666;
  margin-bottom: 1.5rem;
}}
</style>
"#,
        name = ctx.name,
    )
}

pub fn about_page_vue(ctx: &Ctx) -> String {
    format!(
        r#"<script setup lang="ts">
import {{ RouterLink }} from 'vue-router';
</script>

<template>
  <div class="about">
    <h1>About {name}</h1>
    <p>
      This project was scaffolded with <strong>mg</strong> using the Vue template.
    </p>
    <RouterLink to="/">Back to home</RouterLink>
  </div>
</template>
"#,
        name = ctx.name,
    )
}

pub fn app_button_vue() -> String {
    r#"<script setup lang="ts">
import { computed } from 'vue';

type Variant = 'primary' | 'secondary' | 'outline';

interface Props {
  variant?: Variant;
  class?: string;
  disabled?: boolean;
  type?: 'button' | 'submit' | 'reset';
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'primary',
  type: 'button',
});

const variantStyles: Record<Variant, string> = {
  primary: 'btn btn--primary',
  secondary: 'btn btn--secondary',
  outline: 'btn btn--outline',
};

const className = computed(() =>
  [variantStyles[props.variant], props.class].filter(Boolean).join(' '),
);
</script>

<template>
  <button
    :class="className"
    :disabled="props.disabled"
    :type="props.type"
    v-bind="$attrs"
  >
    <slot />
  </button>
</template>
"#
    .into()
}

pub fn app_input_vue() -> String {
    r#"<script setup lang="ts">
import { computed } from 'vue';

interface Props {
  id?: string;
  label?: string;
  error?: string;
  type?: string;
  placeholder?: string;
  disabled?: boolean;
  required?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  id: '',
  label: '',
  error: '',
  type: 'text',
  placeholder: '',
  disabled: false,
  required: false,
});

const inputId = computed(() =>
  props.id || props.label?.toLowerCase().replace(/\s+/g, '-'),
);
</script>

<template>
  <div class="field">
    <label v-if="props.label" :for="inputId">{{ props.label }}</label>
    <input
      :id="inputId"
      :type="props.type"
      :disabled="props.disabled"
      :required="props.required"
      :placeholder="props.placeholder"
      :class="['field__input', props.error ? 'field__input--error' : '', $attrs.class]"
      v-bind="$attrs"
    />
    <span v-if="props.error" class="field__error">{{ props.error }}</span>
  </div>
</template>
"#
    .into()
}

pub fn app_card_vue() -> String {
    r#"<script setup lang="ts">
interface Props {
  header?: string;
  footer?: string;
  className?: string;
}

const { header, footer, className = '' } = withDefaults(defineProps<Props>(), {
  className: '',
});
</script>

<template>
  <div :class="['card', className]">
    <div v-if="header" class="card__header">{{ header }}</div>
    <div class="card__content"><slot /></div>
    <div v-if="footer" class="card__footer">{{ footer }}</div>
  </div>
</template>
"#
    .into()
}

pub fn app_header_vue(ctx: &Ctx) -> String {
    format!(
        r#"<script setup lang="ts">
import {{ RouterLink, useRoute }} from 'vue-router';

const route = useRoute();

const links = [
  {{ to: '/', label: 'Home' }},
  {{ to: '/about', label: 'About' }},
];
</script>

<template>
  <header class="header">
    <nav class="header__nav">
      <span class="header__brand">{name}</span>
      <ul class="header__links">
        <li v-for="link in links" :key="link.to">
          <RouterLink
            :to="link.to"
            :class="['header__link', route.path === link.to ? 'header__link--active' : '']"
          >
            {{ link.label }}
          </RouterLink>
        </li>
      </ul>
    </nav>
  </header>
</template>
"#,
        name = ctx.name,
    )
}

pub fn use_auth_ts() -> String {
    r#"import { computed } from 'vue';
import { useAuthStore } from '@/stores/auth';

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
}
"#
    .into()
}

pub fn router_index_ts() -> String {
    r#"import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router';
import HomePage from '@/pages/HomePage.vue';
import AboutPage from '@/pages/AboutPage.vue';

const routes: RouteRecordRaw[] = [
  { path: '/', name: 'Home', component: HomePage },
  { path: '/about', name: 'About', component: AboutPage },
];

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes,
});

export default router;
"#
    .into()
}

pub fn api_ts() -> String {
    r#"const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:3001';

interface RequestOptions extends RequestInit {
  params?: Record<string, string>;
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = 'ApiError';
  }
}

function sanitizeError(status: number, text: string): string {
  const safeStatuses = [400, 401, 403, 404, 422];
  if (safeStatuses.includes(status)) {
    return text;
  }
  return 'An unexpected error occurred';
}

async function request<T>(
  endpoint: string,
  options: RequestOptions = {},
): Promise<T> {
  const { params, ...fetchOptions } = options;
  const url = new URL(`${API_BASE_URL}${endpoint}`);
  if (params) url.search = new URLSearchParams(params).toString();

  const response = await fetch(url.toString(), {
    headers: { 'Content-Type': 'application/json', ...fetchOptions.headers },
    ...fetchOptions,
  });

  if (!response.ok) {
    const raw = await response.text().catch(() => '');
    throw new ApiError(response.status, sanitizeError(response.status, raw));
  }

  return response.json() as Promise<T>;
}

export const api = {
  get: <T>(endpoint: string, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: 'GET' }),

  post: <T>(endpoint: string, body: unknown, options?: RequestOptions) =>
    request<T>(endpoint, {
      ...options,
      method: 'POST',
      body: JSON.stringify(body),
    }),

  put: <T>(endpoint: string, body: unknown, options?: RequestOptions) =>
    request<T>(endpoint, {
      ...options,
      method: 'PUT',
      body: JSON.stringify(body),
    }),

  delete: <T>(endpoint: string, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: 'DELETE' }),
};
"#
    .into()
}

pub fn auth_store_ts() -> String {
    r#"import { ref } from 'vue';
import { defineStore } from 'pinia';
import type { User } from '@/types';

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
});
"#
    .into()
}

pub fn types_index_ts() -> String {
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
}
"#
    .into()
}

pub fn helpers_ts() -> String {
    r#"export function cn(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(' ');
}

export function formatDate(date: Date, locale = 'en-US'): string {
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  }).format(date);
}

export function truncate(str: string, maxLength: number): string {
  if (str.length <= maxLength) return str;
  return `${str.slice(0, maxLength)}...`;
}
"#
    .into()
}

pub fn main_css() -> String {
    r#"*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

:root {
  --color-primary: #42b883;
  --color-primary-hover: #3aa876;
  --color-secondary: #6c757d;
  --color-bg: #ffffff;
  --color-bg-muted: #f8f9fa;
  --color-text: #2c3e50;
  --color-text-muted: #6c757d;
  --color-border: #dee2e6;
  --color-error: #e53e3e;
  --radius: 8px;
  --spacing: 16px;
}

html {
  font-family: Inter, system-ui, -apple-system, sans-serif;
  color: var(--color-text);
  background: var(--color-bg);
  line-height: 1.5;
}

a {
  color: var(--color-primary);
  text-decoration: none;
}

a:hover {
  color: var(--color-primary-hover);
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid transparent;
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.2s, border-color 0.2s;
}

.btn--primary {
  background: var(--color-primary);
  color: #fff;
}

.btn--primary:hover {
  background: var(--color-primary-hover);
}

.btn--secondary {
  background: var(--color-secondary);
  color: #fff;
}

.btn--outline {
  background: transparent;
  border-color: var(--color-border);
  color: var(--color-text);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field__input {
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
  padding: 8px 12px;
  font-size: 14px;
}

.field__input--error {
  border-color: var(--color-error);
}

.field__error {
  color: var(--color-error);
  font-size: 12px;
}

.card {
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
  background: var(--color-bg);
}

.card__header {
  padding: var(--spacing);
  border-bottom: 1px solid var(--color-border);
  font-weight: 600;
}

.card__content {
  padding: var(--spacing);
}

.card__footer {
  padding: var(--spacing);
  border-top: 1px solid var(--color-border);
}

.header {
  border-bottom: 1px solid var(--color-border);
  padding: var(--spacing);
  background: var(--color-bg-muted);
}

.header__nav {
  display: flex;
  align-items: center;
  gap: 24px;
  max-width: 960px;
  margin: 0 auto;
}

.header__brand {
  font-weight: 700;
  font-size: 18px;
}

.header__links {
  list-style: none;
  display: flex;
  gap: 16px;
  margin-left: auto;
}

.header__link {
  color: var(--color-text-muted);
  padding: 4px 8px;
  border-radius: var(--radius);
}

.header__link--active {
  color: var(--color-primary);
  font-weight: 500;
}

.app {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

main {
  flex: 1;
  max-width: 960px;
  width: 100%;
  margin: 0 auto;
  padding: 32px var(--spacing);
}

.home,
.about {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding-top: 64px;
  text-align: center;
}
"#
    .into()
}

pub fn gitignore() -> String {
    r#"node_modules/
dist/
.vite/
*.tsbuildinfo
.DS_Store
.env
*.local
"#
    .into()
}

pub fn env_example() -> String {
    "VITE_API_URL=http://localhost:3001\n".into()
}

pub fn readme(ctx: &Ctx) -> String {
    let pascal = ctx.name.to_upper_camel_case();
    format!(
        r#"# {pascal}

> Vue 3 SPA built with Vite + TypeScript.

Built with [mg](https://mgpm.dev) + Vue 3 + TypeScript + Vite + Pinia + Vue Router.

## Getting Started

```bash
npm install
npm run dev
```

Open [http://localhost:4315](http://localhost:4315).

## Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | Start dev server |
| `npm run build` | Type-check and build for production |
| `npm run preview` | Preview production build |
| `npm run lint` | Run ESLint |
| `npm run format` | Format with Prettier |

## Project Structure

```
src/
├── components/
│   ├── ui/        # Reusable UI primitives
│   └── features/  # Feature-specific components
├── composables/   # Vue composables
├── pages/         # Route pages
├── router/        # Vue Router setup
├── services/      # API client
├── stores/        # Pinia stores
├── types/         # TypeScript type definitions
└── utils/         # Utility functions
```
"#,
        pascal = pascal,
    )
}
