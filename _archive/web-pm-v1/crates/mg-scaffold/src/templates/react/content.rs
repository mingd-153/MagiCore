use crate::versions::*;
use heck::ToUpperCamelCase;

pub struct Ctx {
    pub name: String,
    pub version: String,
    pub has_ts: bool,
    pub has_tailwind: bool,
}

impl Ctx {
    pub fn new(name: &str, version: &str, has_ts: bool) -> Self {
        Self { name: name.to_string(), version: version.to_string(), has_ts, has_tailwind: false }
    }
    pub fn ext(&self) -> &'static str {
        if self.has_ts { "tsx" } else { "jsx" }
    }
    pub fn ext_raw(&self) -> &'static str {
        if self.has_ts { "ts" } else { "js" }
    }
    pub fn ts_label(&self) -> &'static str {
        if self.has_ts { " + TypeScript" } else { "" }
    }
    pub fn src_ext(&self) -> &'static str {
        if self.has_ts { ".tsx" } else { ".jsx" }
    }
    fn pascal(&self) -> String {
        self.name.to_upper_camel_case()
    }
}

pub fn package_json(ctx: &Ctx) -> String {
    let build = if ctx.has_ts { "tsc -b && vite build" } else { "vite build" };
    let mut devs = format!(
        r#""@vitejs/plugin-react":"{}","prettier":"{}","vite":"{}""#,
        VITE_PLUGIN_REACT(), PRETTIER(), VITE(),
    );
    if ctx.has_ts {
        devs = format!(
            r#""@eslint/js":"{}","@types/node":"{}","@types/react":"{}","@types/react-dom":"{}",{},"eslint":"{}","eslint-plugin-react":"{}","eslint-plugin-react-hooks":"{}","eslint-plugin-react-refresh":"{}","globals":"{}","typescript":"{}","typescript-eslint":"{}""#,
            ESLINT_JS(), TYPES_NODE(), TYPES_REACT(), TYPES_REACT_DOM(),
            devs,
            ESLINT(), ESLINT_PLUGIN_REACT(), ESLINT_PLUGIN_REACT_HOOKS(), ESLINT_PLUGIN_REACT_REFRESH(),
            GLOBALS(), TYPESCRIPT(), TYPESCRIPT_ESLINT(),
        );
    }
    if ctx.has_tailwind {
        devs = format!(
            r#""tailwindcss":"{}","@tailwindcss/vite":"{}",{}"#,
            TAILWINDCSS(), TAILWINDCSS_VITE(), devs,
        );
    }
    let deps = format!(
        r#""react":"{}","react-dom":"{}","react-router":"{}","zustand":"{}""#,
        REACT(), REACT_DOM(), REACT_ROUTER(), ZUSTAND(),
    );
    let lint = if ctx.has_ts {
        r#""lint":"eslint .","format":"prettier --write .""#
    } else {
        r#""format":"prettier --write .""#
    };

    format!(
        "{{\"name\":\"{name}\",\"private\":true,\"version\":\"{version}\",\"type\":\"module\",\
         \"scripts\":{{\"dev\":\"vite\",\"build\":\"{build}\",\"preview\":\"vite preview\",{lint}}},\
         \"dependencies\":{{{deps}}},\"devDependencies\":{{{devs}}}}}",
        name = ctx.name, version = ctx.version, build = build,
        deps = deps, devs = devs, lint = lint,
    )
}

pub fn index_html(ctx: &Ctx) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"UTF-8\" />\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
         <link rel=\"icon\" href=\"/favicon.ico\" sizes=\"32x32\" />\n\
         <title>{name}</title>\n</head>\n<body>\n\
         <div id=\"root\"></div>\n\
         <script type=\"module\" src=\"/src/main{ext}\"></script>\n\
         </body>\n</html>",
        name = ctx.name, ext = ctx.src_ext()
    )
}

pub fn readme(ctx: &Ctx) -> String {
    let pascal = ctx.pascal();
    let ts = ctx.ts_label();
    format!(
        "# {pascal}\n\n> React SPA built with Vite{ts}.\n\n\
         Built with [MG](https://mg.dev) + React + TypeScript + Vite.\n\n\
         ## Getting Started\n\n```bash\nmg install\nmg run dev\n```\n\n\
          Open [http://localhost:4315](http://localhost:4315).\n\n\
         ## Scripts\n\n| Command | Description |\n\
         |---------|-------------|\n\
         | `mg run dev` | Start dev server |\n\
         | `mg run build` | Build for production |\n\
         | `mg run preview` | Preview production build |\n\
         | `mg run format` | Format with Prettier |\n\n\
         ## Project Structure\n\n```\nsrc/\n├── components/\n\
         │   ├── ui/        # Reusable UI primitives\n\
         │   └── features/  # Feature-specific components\n\
         ├── hooks/         # Custom React hooks\n\
         ├── pages/         # Route pages\n\
         ├── services/      # API client\n\
         ├── stores/        # Zustand state stores\n\
         ├── types/         # TypeScript type definitions\n\
         └── utils/         # Utility functions\n```"
    )
}

pub fn main_content(ctx: &Ctx) -> String {
    let ts_assert = if ctx.has_ts { " as HTMLElement" } else { "" };
    format!(
        "import {{ StrictMode }} from 'react';\n\
         import {{ createRoot }} from 'react-dom/client';\n\
         import {{ BrowserRouter }} from 'react-router';\n\
         import App from './App';\n\
         import './styles/globals.css';\n\n\
         const root = document.getElementById('root'){ts_assert};\n\
         if (!root) {{\n  throw new Error('Root element not found');\n}}\n\n\
         createRoot(root).render(\n  <StrictMode>\n\
         <BrowserRouter>\n      <App />\n    </BrowserRouter>\n  </StrictMode>,\n);\n"
    )
}

pub fn app_content() -> String {
    "import { Routes, Route } from 'react-router';\n\
     import Header from '@/components/features/header';\n\
     import Home from '@/pages/home';\n\
     import About from '@/pages/about';\n\n\
     export default function App() {\n  return (\n\
     <div className=\"app\">\n      <Header />\n      <main>\n\
     <Routes>\n        <Route path=\"/\" element={<Home />} />\n\
     <Route path=\"/about\" element={<About />} />\n\
     </Routes>\n      </main>\n    </div>\n  );\n}\n"
    .into()
}

pub fn home_content(name: &str) -> String {
    format!(
        "import {{ Link }} from 'react-router';\n\n\
         export default function Home() {{\n  return (\n    <div className=\"home\">\n\
         <h1>Welcome to {name}</h1>\n      <p>Built with React + TypeScript + Vite</p>\n\
         <Link to=\"/about\">Learn more</Link>\n    </div>\n  );\n}}"
    )
}

pub fn about_content(name: &str) -> String {
    format!(
        "import {{ Link }} from 'react-router';\n\n\
         export default function About() {{\n  return (\n    <div className=\"about\">\n\
         <h1>About {name}</h1>\n      <p>\n\
          This project was scaffolded with <strong>mg</strong> using the React\n\
         template.\n      </p>\n      <Link to=\"/\">Back to home</Link>\n    </div>\n  );\n}}"
    )
}

pub fn header_content(name: &str) -> String {
    format!(
        "import {{ NavLink }} from 'react-router';\n\n\
         const links = [\n  {{ to: '/', label: 'Home' }},\n  {{ to: '/about', label: 'About' }},\n];\n\n\
         export default function Header() {{\n  return (\n    <header className=\"header\">\n\
         <nav className=\"header__nav\">\n\
         <span className=\"header__brand\">{name}</span>\n\
         <ul className=\"header__links\">\n\
         {{links.map((link) => (\n              <li key={{link.to}}>\n\
         <NavLink\n                to={{link.to}}\n\
         className={{({{ isActive }}) =>\n\
                isActive ? 'header__link header__link--active' : 'header__link'\n\
              }}\n              >\n\
         {{link.label}}\n              </NavLink>\n            </li>\n\
         ))}}\n        </ul>\n      </nav>\n    </header>\n  );\n}}"
    )
}

pub fn button_jsx() -> String {
    "const variantStyles = {\n\
     primary: 'btn btn--primary',\n\
     secondary: 'btn btn--secondary',\n\
     outline: 'btn btn--outline',\n};\n\n\
     export default function Button({ variant = 'primary', className = '', children, ...props }) {\n\
     return (\n    <button className={`${variantStyles[variant]} ${className}`.trim()} {...props}>\n\
     {children}\n    </button>\n  );\n}\n"
    .into()
}

pub fn button_tsx() -> String {
    "import { type ButtonHTMLAttributes, type ReactNode } from 'react';\n\n\
     type Variant = 'primary' | 'secondary' | 'outline';\n\n\
     interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {\n\
     variant?: Variant;\n  children: ReactNode;\n}\n\n\
     const variantStyles: Record<Variant, string> = {\n\
     primary: 'btn btn--primary',\n\
     secondary: 'btn btn--secondary',\n\
     outline: 'btn btn--outline',\n};\n\n\
     export default function Button({ variant = 'primary', className = '', children, ...props }: ButtonProps) {\n\
     return (\n    <button className={`${variantStyles[variant]} ${className}`.trim()} {...props}>\n\
     {children}\n    </button>\n  );\n}\n"
    .into()
}

pub fn input_jsx() -> String {
    "import { forwardRef } from 'react';\n\n\
     const Input = forwardRef(({ label, error, className = '', id, ...props }, ref) => {\n\
     const inputId = id || label?.toLowerCase().replace(/\\s+/g, '-');\n\
     return (\n      <div className=\"field\">\n\
     {label && <label htmlFor={inputId}>{label}</label>}\n\
     <input ref={ref} id={inputId}\n\
     className={`field__input ${error ? 'field__input--error' : ''} ${className}`.trim()}\n\
     {...props} />\n\
     {error && <span className=\"field__error\">{error}</span>}\n\
     </div>\n    );\n  });\n\nInput.displayName = 'Input';\n\nexport default Input;\n"
    .into()
}

pub fn input_tsx() -> String {
    "import { type InputHTMLAttributes, forwardRef } from 'react';\n\n\
     interface InputProps extends InputHTMLAttributes<HTMLInputElement> {\n\
     label?: string;\n  error?: string;\n}\n\n\
     const Input = forwardRef<HTMLInputElement, InputProps>(\n\
     ({ label, error, className = '', id, ...props }, ref) => {\n\
     const inputId = id || label?.toLowerCase().replace(/\\s+/g, '-');\n\
     return (\n      <div className=\"field\">\n\
     {label && <label htmlFor={inputId}>{label}</label>}\n\
     <input ref={ref} id={inputId}\n\
     className={`field__input ${error ? 'field__input--error' : ''} ${className}`.trim()}\n\
     {...props} />\n\
     {error && <span className=\"field__error\">{error}</span>}\n\
     </div>\n    );\n  },\n);\n\nInput.displayName = 'Input';\n\nexport default Input;\n"
    .into()
}

pub fn card_jsx() -> String {
    "export default function Card({ header, children, footer, className = '' }) {\n\
     return (\n    <div className={`card ${className}`.trim()}>\n\
     {header && <div className=\"card__header\">{header}</div>}\n\
     <div className=\"card__content\">{children}</div>\n\
     {footer && <div className=\"card__footer\">{footer}</div>}\n    </div>\n  );\n}\n"
    .into()
}

pub fn card_tsx() -> String {
    "import { type ReactNode } from 'react';\n\n\
     interface CardProps {\n  header?: ReactNode;\n\
     children: ReactNode;\n  footer?: ReactNode;\n  className?: string;\n}\n\n\
     export default function Card({ header, children, footer, className = '' }: CardProps) {\n\
     return (\n    <div className={`card ${className}`.trim()}>\n\
     {header && <div className=\"card__header\">{header}</div>}\n\
     <div className=\"card__content\">{children}</div>\n\
     {footer && <div className=\"card__footer\">{footer}</div>}\n    </div>\n  );\n}\n"
    .into()
}

pub fn use_auth_js() -> String {
    "import { useCallback } from 'react';\n\
     import { useAuthStore } from '../stores/auth-store';\n\n\
     export function useAuth() {\n\
     const { user, isAuthenticated, login, logout } = useAuthStore();\n\n\
     const handleLogin = useCallback(async (email, password) => {\n\
     await login(email, password);\n  }, [login]);\n\n\
     const handleLogout = useCallback(() => {\n    logout();\n  }, [logout]);\n\n\
     return { user, isAuthenticated, login: handleLogin, logout: handleLogout };\n}\n"
    .into()
}

pub fn use_auth_ts() -> String {
    "import { useCallback } from 'react';\n\
     import { useAuthStore } from '../stores/auth-store';\n\n\
     export function useAuth() {\n\
     const { user, isAuthenticated, login, logout } = useAuthStore();\n\n\
     const handleLogin = useCallback(\n\
     async (email: string, password: string) => {\n\
     await login(email, password);\n    },\n    [login],\n  );\n\n\
     const handleLogout = useCallback(() => {\n    logout();\n  }, [logout]);\n\n\
     return { user, isAuthenticated, login: handleLogin, logout: handleLogout };\n}\n"
    .into()
}

pub fn api_js() -> String {
    "const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:3001';\n\n\
     export class ApiError extends Error {\n\
     constructor(status, message) {\n\
     super(message);\n    this.name = 'ApiError';\n    this.status = status;\n  }\n}\n\n\
     function sanitizeError(status, text) {\n\
     const safeStatuses = [400, 401, 403, 404, 422];\n\
     if (safeStatuses.includes(status)) return text;\n\
     return 'An unexpected error occurred';\n}\n\n\
     export async function request(endpoint, options = {}) {\n\
     const { params, ...fetchOptions } = options;\n\
     const url = new URL(`${API_BASE_URL}${endpoint}`);\n\
     if (params) url.search = new URLSearchParams(params).toString();\n\
     const res = await fetch(url.toString(), {\n\
     headers: { 'Content-Type': 'application/json', ...fetchOptions.headers },\n\
     ...fetchOptions,\n    });\n\
     if (!res.ok) {\n\
     const raw = await res.text().catch(() => '');\n\
     throw new ApiError(res.status, sanitizeError(res.status, raw));\n    }\n\
     return res.json();\n}\n\n\
     export const api = {\n\
     get: (endpoint, options) => request(endpoint, { ...options, method: 'GET' }),\n\
     post: (endpoint, body, options) => request(endpoint, { ...options, method: 'POST', body: JSON.stringify(body) }),\n\
     put: (endpoint, body, options) => request(endpoint, { ...options, method: 'PUT', body: JSON.stringify(body) }),\n\
     delete: (endpoint, options) => request(endpoint, { ...options, method: 'DELETE' }),\n};\n"
    .into()
}

pub fn api_ts() -> String {
    "const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:3001';\n\n\
     interface RequestOptions extends RequestInit {\n  params?: Record<string, string>;\n}\n\n\
     export class ApiError extends Error {\n\
     constructor(public status: number, message: string) {\n\
     super(message);\n    this.name = 'ApiError';\n  }\n}\n\n\
     function sanitizeError(status: number, text: string): string {\n\
     const safeStatuses = [400, 401, 403, 404, 422];\n\
     if (safeStatuses.includes(status)) return text;\n\
     return 'An unexpected error occurred';\n}\n\n\
     export async function request<T>(endpoint: string, options: RequestOptions = {}): Promise<T> {\n\
     const { params, ...fetchOptions } = options;\n\
     const url = new URL(`${API_BASE_URL}${endpoint}`);\n\
     if (params) url.search = new URLSearchParams(params).toString();\n\
     const res = await fetch(url.toString(), {\n\
     headers: { 'Content-Type': 'application/json', ...fetchOptions.headers },\n\
     ...fetchOptions,\n    });\n\
     if (!res.ok) {\n\
     const raw = await res.text().catch(() => '');\n\
     throw new ApiError(res.status, sanitizeError(res.status, raw));\n    }\n\
     return res.json() as Promise<T>;\n}\n\n\
     export const api = {\n\
     get: <T>(endpoint: string, options?: RequestOptions) => request<T>(endpoint, { ...options, method: 'GET' }),\n\
     post: <T>(endpoint: string, body: unknown, options?: RequestOptions) => request<T>(endpoint, { ...options, method: 'POST', body: JSON.stringify(body) }),\n\
     put: <T>(endpoint: string, body: unknown, options?: RequestOptions) => request<T>(endpoint, { ...options, method: 'PUT', body: JSON.stringify(body) }),\n\
     delete: <T>(endpoint: string, options?: RequestOptions) => request<T>(endpoint, { ...options, method: 'DELETE' }),\n};\n"
    .into()
}

pub fn auth_store_js() -> String {
    "import { create } from 'zustand';\n\n\
     export const useAuthStore = create((set) => ({\n\
     user: null,\n  isAuthenticated: false,\n\
     login: async (email, _password) => {\n\
     set({ user: { id: '1', email }, isAuthenticated: true });\n  },\n\
     logout: () => set({ user: null, isAuthenticated: false }),\n}));\n"
    .into()
}

pub fn auth_store_ts() -> String {
    "import { create } from 'zustand';\n\
     import type { User } from '../types';\n\n\
     interface AuthState {\n  user: User | null;\n\
     isAuthenticated: boolean;\n\
     login: (email: string, password: string) => Promise<void>;\n  logout: () => void;\n}\n\n\
     export const useAuthStore = create<AuthState>((set) => ({\n\
     user: null,\n  isAuthenticated: false,\n\
     // TODO: Replace with real authentication (call API, handle tokens, etc.)\n\
     login: async (email: string, _password: string) => {\n\
     set({ user: { id: '1', email }, isAuthenticated: true });\n  },\n\
     logout: () => set({ user: null, isAuthenticated: false }),\n}));\n"
    .into()
}

pub fn types_ts() -> String {
    "export interface User {\n  id: string;\n  email: string;\n  name?: string;\n}\n\n\
     export interface ApiResponse<T> {\n  data: T;\n  message?: string;\n}\n\n\
     export interface PaginatedResponse<T> {\n  data: T[];\n\
     total: number;\n  page: number;\n  pageSize: number;\n  totalPages: number;\n}\n"
    .into()
}

pub fn helpers_js() -> String {
    "export function cn(...classes) {\n\
     return classes.filter(Boolean).join(' ');\n}\n\n\
     export function formatDate(date, locale = 'en-US') {\n\
     return new Intl.DateTimeFormat(locale, {\n\
     year: 'numeric', month: 'long', day: 'numeric',\n  }).format(date);\n}\n\n\
     export function truncate(str, maxLength) {\n\
     if (str.length <= maxLength) return str;\n\
     return `${str.slice(0, maxLength)}...`;\n}\n"
    .into()
}

pub fn helpers_ts() -> String {
    "export function cn(...classes: (string | false | null | undefined)[]): string {\n\
     return classes.filter(Boolean).join(' ');\n}\n\n\
     export function formatDate(date: Date, locale = 'en-US'): string {\n\
     return new Intl.DateTimeFormat(locale, {\n\
     year: 'numeric', month: 'long', day: 'numeric',\n  }).format(date);\n}\n\n\
     export function truncate(str: string, maxLength: number): string {\n\
     if (str.length <= maxLength) return str;\n\
     return `${str.slice(0, maxLength)}...`;\n}\n"
    .into()
}

pub fn vite_config_js(ctx: &Ctx) -> String {
    let tailwind_import = if ctx.has_tailwind {
        "import tailwindcss from '@tailwindcss/vite';\n"
    } else {
        ""
    };
    let plugins = if ctx.has_tailwind {
        "plugins: [react(), tailwindcss()],"
    } else {
        "plugins: [react()],"
    };
    format!(
        "import {{ defineConfig }} from 'vite';\n\
         import react from '@vitejs/plugin-react';\n\
         import path from 'node:path';\n\
         {tailwind_import}\
         export default defineConfig({{\n\
         {plugins}\n\
         resolve: {{\n\
         alias: {{ '@': path.resolve(import.meta.dirname, './src') }},\n  }},\n\
         server: {{\n    port: 4315,\n    open: true,\n  }},\n\
         preview: {{\n    port: 4316,\n  }},\n\
         build: {{\n    target: 'es2022',\n  }},\n}});\n"
    )
}

pub fn vite_config_ts(ctx: &Ctx) -> String {
    vite_config_js(ctx)
}

pub fn tsconfig_json() -> String {
    "{\n  \"files\": [],\n  \"references\": [\n\
     { \"path\": \"./tsconfig.app.json\" },\n\
     { \"path\": \"./tsconfig.node.json\" }\n  ]\n}\n"
    .into()
}

pub fn tsconfig_app_json() -> String {
    "{\n  \"compilerOptions\": {\n\
     \"target\": \"ES2020\",\n  \"useDefineForClassFields\": true,\n\
     \"lib\": [\"ES2020\", \"DOM\", \"DOM.Iterable\"],\n\
     \"module\": \"ESNext\",\n  \"skipLibCheck\": true,\n\
     \"moduleResolution\": \"bundler\",\n\
     \"allowImportingTsExtensions\": true,\n\
     \"isolatedModules\": true,\n  \"moduleDetection\": \"force\",\n\
     \"noEmit\": true,\n  \"jsx\": \"react-jsx\",\n\
     \"strict\": true,\n  \"noUnusedLocals\": true,\n\
     \"noUnusedParameters\": true,\n\
     \"noFallthroughCasesInSwitch\": true,\n\
     \"noUncheckedIndexedAccess\": true,\n\
     \"paths\": {\n      \"@/*\": [\"./src/*\"]\n    }\n  },\n\
     \"include\": [\"src\"]\n}\n"
    .into()
}

pub fn tsconfig_node_json() -> String {
    "{\n  \"compilerOptions\": {\n\
     \"target\": \"ES2022\",\n  \"lib\": [\"ES2023\"],\n\
     \"module\": \"ESNext\",\n  \"skipLibCheck\": true,\n\
     \"moduleResolution\": \"bundler\",\n\
     \"allowImportingTsExtensions\": true,\n\
     \"isolatedModules\": true,\n  \"moduleDetection\": \"force\",\n\
     \"noEmit\": true,\n  \"strict\": true,\n\
     \"noUnusedLocals\": true,\n\
     \"noUnusedParameters\": true,\n\
     \"noFallthroughCasesInSwitch\": true\n  },\n\
     \"include\": [\"vite.config.ts\"]\n}\n"
    .into()
}

pub fn eslint_config() -> String {
    "import js from '@eslint/js';\n\
     import tseslint from 'typescript-eslint';\n\
     import react from 'eslint-plugin-react';\n\
     import reactHooks from 'eslint-plugin-react-hooks';\n\
     import reactRefresh from 'eslint-plugin-react-refresh';\n\
     import globals from 'globals';\n\n\
     export default tseslint.config(\n  { ignores: ['dist'] },\n  {\n\
     extends: [js.configs.recommended, ...tseslint.configs.recommended],\n\
     files: ['**/*.{ts,tsx}'],\n\
     languageOptions: {\n\
     ecmaVersion: 2020,\n\
     globals: globals.browser,\n\
     parserOptions: { ecmaFeatures: { jsx: true } },\n    },\n\
     plugins: {\n      react,\n      'react-hooks': reactHooks,\n\
     'react-refresh': reactRefresh,\n    },\n\
     rules: {\n      ...react.configs.recommended.rules,\n\
     ...reactHooks.configs.recommended.rules,\n\
     'react/jsx-no-target-blank': 'off',\n\
     'react/react-in-jsx-scope': 'off',\n\
     'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],\n\
     '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],\n    },\n\
     settings: { react: { version: '19.0' } },\n  },\n);\n"
    .into()
}

pub fn vite_env_dts() -> String {
    "/// <reference types=\"vite/client\" />\n\n\
     interface ImportMetaEnv {\n  readonly VITE_API_URL: string;\n}\n\n\
     interface ImportMeta {\n  readonly env: ImportMetaEnv;\n}\n"
    .into()
}

pub fn globals_css(ctx: &Ctx) -> String {
    if ctx.has_tailwind {
        return "@import \"tailwindcss\";\n".into();
    }
    "*,\n*::before,\n*::after { box-sizing: border-box; margin: 0; padding: 0; }\n\n\
     :root {\n  --color-primary: #646cff;\n  --color-primary-hover: #535bf2;\n\
     --color-secondary: #6c757d;\n  --color-bg: #ffffff;\n  --color-bg-muted: #f8f9fa;\n\
     --color-text: #213547;\n  --color-text-muted: #6c757d;\n  --color-border: #dee2e6;\n\
     --color-error: #e53e3e;\n  --radius: 8px;\n  --spacing: 16px;\n}\n\n\
     html {\n  font-family: Inter, system-ui, -apple-system, sans-serif;\n\
     color: var(--color-text);\n  background: var(--color-bg);\n  line-height: 1.5;\n}\n\n\
     a { color: var(--color-primary); text-decoration: none; }\n\
     a:hover { color: var(--color-primary-hover); }\n\n\
     .btn {\n  display: inline-flex; align-items: center; justify-content: center;\n\
     border: 1px solid transparent; border-radius: var(--radius);\n\
     padding: 8px 16px; font-size: 14px; font-weight: 500;\n\
     cursor: pointer; transition: background-color 0.2s, border-color 0.2s;\n}\n\n\
     .btn--primary { background: var(--color-primary); color: #fff; }\n\
     .btn--primary:hover { background: var(--color-primary-hover); }\n\
     .btn--secondary { background: var(--color-secondary); color: #fff; }\n\
     .btn--outline {\n  background: transparent;\n  border-color: var(--color-border);\n  color: var(--color-text);\n}\n\n\
     .field { display: flex; flex-direction: column; gap: 4px; }\n\
     .field__input {\n  border: 1px solid var(--color-border);\n\
     border-radius: var(--radius); padding: 8px 12px; font-size: 14px;\n}\n\
     .field__input--error { border-color: var(--color-error); }\n\
     .field__error { color: var(--color-error); font-size: 12px; }\n\n\
     .card { border: 1px solid var(--color-border); border-radius: var(--radius); background: var(--color-bg); }\n\
     .card__header { padding: var(--spacing); border-bottom: 1px solid var(--color-border); font-weight: 600; }\n\
     .card__content { padding: var(--spacing); }\n\
     .card__footer { padding: var(--spacing); border-top: 1px solid var(--color-border); }\n\n\
     .header { border-bottom: 1px solid var(--color-border); padding: var(--spacing); background: var(--color-bg-muted); }\n\
     .header__nav { display: flex; align-items: center; gap: 24px; max-width: 960px; margin: 0 auto; }\n\
     .header__brand { font-weight: 700; font-size: 18px; }\n\
     .header__links { list-style: none; display: flex; gap: 16px; margin-left: auto; }\n\
     .header__link { color: var(--color-text-muted); padding: 4px 8px; border-radius: var(--radius); }\n\
     .header__link--active { color: var(--color-primary); font-weight: 500; }\n\n\
     .app { min-height: 100vh; display: flex; flex-direction: column; }\n\n\
     main { flex: 1; max-width: 960px; width: 100%; margin: 0 auto; padding: 32px var(--spacing); }\n\n\
     .home, .about { display: flex; flex-direction: column; align-items: center; gap: 16px; padding-top: 64px; text-align: center; }\n"
    .into()
}

pub fn gitignore() -> String {
    "node_modules/\ndist/\n.vite/\n*.tsbuildinfo\n.DS_Store\n.env\n*.local\n".into()
}

pub fn env_example() -> String {
    "VITE_API_URL=http://localhost:3001\n".into()
}


