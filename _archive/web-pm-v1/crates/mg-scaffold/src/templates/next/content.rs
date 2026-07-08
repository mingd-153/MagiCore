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
    let deps = format!(
        r#""next":"{}","react":"{}","react-dom":"{}","clsx":"{}","tailwind-merge":"{}""#,
        NEXT(), REACT(), REACT_DOM(),
        CLSX(), TAILWIND_MERGE(),
    );
    let devs = format!(
        r#""typescript":"{}","@types/node":"{}","@types/react":"{}","@types/react-dom":"{}","tailwindcss":"{}","@tailwindcss/postcss":"{}","postcss":"{}","autoprefixer":"{}","eslint":"{}","@eslint/js":"{}","typescript-eslint":"{}","@next/eslint-plugin-next":"{}","prettier":"{}","prettier-plugin-tailwindcss":"{}""#,
        TYPESCRIPT(), TYPES_NODE(), TYPES_REACT(), TYPES_REACT_DOM(),
        TAILWINDCSS(), TAILWINDCSS_POSTCSS(),
        POSTCSS(), AUTOPREFIXER(),
        ESLINT(), ESLINT_JS(), TYPESCRIPT_ESLINT(),
        NEXT_ESLINT_PLUGIN(), PRETTIER(), PRETTIER_PLUGIN_TAILWINDCSS(),
    );
    format!(
        "{{\"name\":\"{name}\",\"version\":\"{version}\",\"private\":true,\
         \"scripts\":{{\"dev\":\"next dev\",\"build\":\"next build\",\"start\":\"next start\",\
         \"lint\":\"next lint\",\"format\":\"prettier --write .\",\"typecheck\":\"tsc --noEmit\"}},\
         \"dependencies\":{{{deps}}},\"devDependencies\":{{{devs}}}}}",
        name = ctx.name, version = ctx.version,
    )
}

pub fn next_config() -> String {
    "import type { NextConfig } from 'next';\n\n\
     const nextConfig: NextConfig = {};\n\n\
     export default nextConfig;\n"
    .into()
}

pub fn tsconfig_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2017",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": true,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "preserve",
    "incremental": true,
    "plugins": [{ "name": "next" }],
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}"#
    .into()
}

pub fn postcss_config() -> String {
    "const config = {\n\
     plugins: {\n\
     '@tailwindcss/postcss': {},\n   },\n};\n\
     export default config;\n"
    .into()
}

pub fn tailwind_config() -> String {
    "import type { Config } from 'tailwindcss';\n\n\
     const config: Config = {\n\
     content: [\n\
     './src/pages/**/*.{js,ts,jsx,tsx,mdx}',\n\
     './src/components/**/*.{js,ts,jsx,tsx,mdx}',\n\
     './src/app/**/*.{js,ts,jsx,tsx,mdx}',\n   ],\n\
     theme: {\n\
     extend: {\n\
     colors: {\n\
     primary: { 500: '#3b82f6' },\n     },\n   },\n },\n\
     plugins: [],\n};\n\
     export default config;\n"
    .into()
}

pub fn eslint_config() -> String {
    "import { dirname } from 'path';\n\
     import { fileURLToPath } from 'url';\n\
     import { FlatCompat } from '@eslint/eslintrc';\n\n\
     const __filename = fileURLToPath(import.meta.url);\n\
     const __dirname = dirname(__filename);\n\n\
     const compat = new FlatCompat({ baseDirectory: __dirname });\n\n\
     const eslintConfig = [\n\
     ...compat.extends('next/core-web-vitals', 'next/typescript'),\n\
     {\n\
     rules: {\n\
     '@next/next/no-html-link-for-pages': 'off',\n\
     },\n },\n];\n\n\
     export default eslintConfig;\n"
    .into()
}

pub fn gitignore() -> String {
    "# dependencies\n/node_modules\n/.pnp\n.pnp.*\n\n\
     # testing\n/coverage\n\n\
     # next.js\n/.next/\n/out/\n\n\
     # production\n/build\n\n\
     # misc\n.DS_Store\n*.pem\n\n\
     # debug\nnpm-debug.log*\nyarn-debug.log*\nyarn-error.log*\n\n\
     # local env files\n.env*.local\n\n\
     # vercel\n.vercel\n\n\
     # typescript\n*.tsbuildinfo\nnext-env.d.ts\n"
    .into()
}

pub fn env_example() -> String {
    "# App\nNEXT_PUBLIC_APP_URL=http://localhost:3000\n\n\
     # Database\nDATABASE_URL=postgresql://localhost:5432/mydb\n\n\
     # Auth (NextAuth.js)\nAUTH_SECRET=\nAUTH_URL=http://localhost:3000\n\n\
     # API Keys\n"
    .into()
}

pub fn env_local_example() -> String {
    "# Local overrides — never commit this file\n\
     # Copy .env.example to .env.local and fill in values\n\
     NEXT_PUBLIC_APP_URL=http://localhost:3000\n\
     DATABASE_URL=postgresql://localhost:5432/mydb\n"
    .into()
}

pub fn readme(ctx: &Ctx) -> String {
    let app_name = ctx.name.to_upper_camel_case();
    format!(
        "# {app_name}\n\n\
         A modern web application built with Next.js 15, TypeScript, and Tailwind CSS.\n\n\
         ## Getting Started\n\n\
         ```bash\n\
         npm install\n\
         npm run dev\n\
         ```\n\n\
         Open [http://localhost:3000](http://localhost:3000).\n\n\
         ## Scripts\n\n\
         - `npm run dev` — Start dev server\n\
         - `npm run build` — Production build\n\
         - `npm run start` — Start production server\n\
         - `npm run lint` — Run ESLint\n\
         - `npm run format` — Format with Prettier\n\
         - `npm run typecheck` — TypeScript check\n",
        app_name = app_name,
    )
}

pub fn dockerfile() -> String {
    "FROM node:20-alpine AS base\n\n\
     FROM base AS deps\n\
     RUN apk add --no-cache libc6-compat\n\
     WORKDIR /app\n\
     COPY package.json ./\n\
     RUN npm ci\n\n\
     FROM base AS builder\n\
     WORKDIR /app\n\
     COPY --from=deps /app/node_modules ./node_modules\n\
     COPY . .\n\
     RUN npm run build\n\n\
     FROM base AS runner\n\
     WORKDIR /app\n\
     ENV NODE_ENV=production\n\
     RUN addgroup --system --gid 1001 nodejs\n\
     RUN adduser --system --uid 1001 nextjs\n\
     COPY --from=builder /app/public ./public\n\
     COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./\n\
     COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static\n\
     USER nextjs\n\
     EXPOSE 3000\n\
     ENV PORT=3000\n\
     CMD [\"node\", \"server.js\"]\n"
    .into()
}

pub fn ci_yml() -> String {
    "name: CI\n\n\
     on:\n\
     push:\n\
     branches: [main]\n  pull_request:\n\
     branches: [main]\n\n\
     jobs:\n  build:\n\
     runs-on: ubuntu-latest\n\n  steps:\n\
     - uses: actions/checkout@v4\n\
     - uses: actions/setup-node@v4\n\
     with:\n\
     node-version: 20\n  cache: 'npm'\n\n\
     - run: npm ci\n\
     - run: npm run lint\n\
     - run: npm run typecheck\n\
     - run: npm run build\n"
    .into()
}

pub fn home_page(ctx: &Ctx) -> String {
    format!(
        "import Link from 'next/link';\n\n\
         export default function Home() {{\n\
         return (\n    <div className=\"max-w-4xl mx-auto px-4 py-16\">\n\
         <section className=\"text-center mb-16\">\n\
         <h1 className=\"text-5xl font-bold mb-6\">Welcome to {name}</h1>\n\
         <p className=\"text-xl text-gray-600 mb-8 max-w-2xl mx-auto\">\n\
         A modern web application built with Next.js 15, TypeScript, and\n\
         Tailwind CSS.\n        </p>\n\
         <div className=\"flex gap-4 justify-center\">\n\
         <Link href=\"/about\"\n\
         className=\"px-8 py-3 bg-blue-600 text-white rounded-lg \
         hover:bg-blue-700 transition font-medium\">Learn More</Link>\n\
         <Link href=\"/dashboard\"\n\
         className=\"px-8 py-3 border border-gray-300 rounded-lg \
         hover:bg-gray-50 transition font-medium\">Dashboard</Link>\n\
         </div>\n      </section>\n\n\
         <section className=\"grid md:grid-cols-3 gap-8\">\n\
         {{[\n          {{ title: 'Fast', desc: 'Server Components and streaming for instant page loads.' }},\n\
         {{ title: 'Type Safe', desc: 'Full TypeScript support with strict mode enabled.' }},\n\
         {{ title: 'Modern', desc: 'Tailwind CSS v4 with the latest React 19 features.' }},\n\
         ].map(({{ title, desc }}) => (\n\
         <div key={{title}} className=\"p-6 border rounded-xl\">\n\
         <h3 className=\"text-lg font-semibold mb-2\">{{title}}</h3>\n\
         <p className=\"text-gray-600\">{{desc}}</p>\n            </div>\n\
         ))}}\n      </section>\n    </div>\n  );\n}}\n",
        name = ctx.name,
    )
}

pub fn not_found() -> String {
    "import Link from 'next/link';\n\n\
     export default function NotFound() {\n\
     return (\n    <div className=\"min-h-screen flex items-center justify-center\">\n\
     <div className=\"text-center\">\n\
     <h1 className=\"text-6xl font-bold text-gray-300 mb-4\">404</h1>\n\
     <h2 className=\"text-2xl font-semibold mb-2\">Page Not Found</h2>\n\
     <p className=\"text-gray-600 mb-8\">\n\
     The page you&apos;re looking for doesn&apos;t exist.\n        </p>\n\
     <Link href=\"/\"\n\
     className=\"px-6 py-3 bg-blue-600 text-white rounded-lg \
     hover:bg-blue-700 transition\">Go Home</Link>\n\
     </div>\n    </div>\n  );\n}\n"
    .into()
}

pub fn globals_css() -> String {
    "@import 'tailwindcss';\n".into()
}

pub fn about_page(ctx: &Ctx) -> String {
    format!(
        "export default function About() {{\n\
         return (\n    <div className=\"max-w-4xl mx-auto px-4 py-16\">\n\
         <h1 className=\"text-4xl font-bold mb-8\">About {name}</h1>\n\
         <div className=\"prose max-w-none\">\n\
         <p>\n\
         This is a modern web application built with Next.js 15,\n\
         TypeScript, and Tailwind CSS. It uses the App Router with\n\
         route groups for marketing and dashboard sections.\n        </p>\n\
         <h2>Features</h2>\n\
         <ul>\n\
         <li>Next.js 15 with App Router</li>\n\
         <li>TypeScript strict mode</li>\n\
         <li>Tailwind CSS v4</li>\n\
         <li>Server Components</li>\n\
         <li>API routes</li>\n\
         <li>Route groups ((marketing), (dashboard))</li>\n        </ul>\n\
         </div>\n    </div>\n  );\n}}\n",
        name = ctx.name,
    )
}

pub fn dashboard_layout() -> String {
    "export default function DashboardLayout({\n  children,\n}: {\n\
     children: React.ReactNode;\n}) {\n\
     return (\n    <div className=\"min-h-screen bg-gray-50\">\n\
     <nav className=\"bg-white border-b px-6 py-3\">\n\
     <div className=\"max-w-7xl mx-auto flex items-center gap-4\">\n\
     <span className=\"font-semibold\">Dashboard</span>\n\
     </div>\n      </nav>\n\
     <main className=\"max-w-7xl mx-auto px-6 py-8\">{children}</main>\n\
     </div>\n  );\n}\n"
    .into()
}

pub fn dashboard_page() -> String {
    "export default function Dashboard() {\n\
     return (\n    <div>\n\
     <h1 className=\"text-3xl font-bold mb-6\">Dashboard</h1>\n\
     <div className=\"grid md:grid-cols-2 lg:grid-cols-3 gap-6\">\n\
     {[\n      { label: 'Users', value: '\u{2014}' },\n\
     { label: 'Revenue', value: '\u{2014}' },\n\
     { label: 'Active Sessions', value: '\u{2014}' },\n\
     ].map(({ label, value }) => (\n\
     <div key={label} className=\"p-6 bg-white rounded-xl border\">\n\
     <p className=\"text-sm text-gray-500 mb-1\">{label}</p>\n\
     <p className=\"text-3xl font-bold\">{value}</p>\n            </div>\n\
     ))}\n      </div>\n    </div>\n  );\n}\n"
    .into()
}

pub fn root_layout(ctx: &Ctx) -> String {
    format!(
        "import type {{ Metadata }} from 'next';\n\
         import './globals.css';\n\n\
         export const metadata: Metadata = {{\n\
         title: '{name}',\n\
         description: 'A modern web application',\n}};\n\n\
         export default function RootLayout({{\n  children,\n}}: {{\n\
         children: React.ReactNode;\n}}) {{\n\
         return (\n    <html lang=\"en\">\n\
         <body>{{children}}</body>\n    </html>\n  );\n}}\n",
        name = ctx.name,
    )
}

pub fn api_hello(ctx: &Ctx) -> String {
    format!(
        "import {{ NextRequest, NextResponse }} from 'next/server';\n\n\
         export async function GET(request: NextRequest) {{\n\
         return NextResponse.json({{ message: 'Hello from {name}!' }});\n}}\n",
        name = ctx.name,
    )
}

pub fn api_auth() -> String {
    "import { NextResponse } from 'next/server';\n\n\
     export async function POST(request: Request) {\n\
     const body = await request.json();\n\
     // TODO: Implement auth logic\n\
     return NextResponse.json({ status: 'ok' });\n}\n\n\
     export async function GET() {\n\
     return NextResponse.json({ authenticated: false });\n}\n"
    .into()
}

pub fn auth_actions() -> String {
    "'use server';\n\n\
     export async function login(formData: FormData) {\n\
     // TODO: Implement login\n\
     return { error: 'Not implemented' };\n}\n\n\
     export async function logout() {\n\
     // TODO: Implement logout\n\
     return { success: true };\n}\n"
    .into()
}

pub fn button() -> String {
    "import { ButtonHTMLAttributes } from 'react';\n\n\
     interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {\n\
     variant?: 'primary' | 'secondary' | 'outline';\n}\n\n\
     export function Button({\n  variant = 'primary',\n  className = '',\n  children,\n  ...props\n}: ButtonProps) {\n\
     const base = 'px-4 py-2 rounded-lg font-medium transition-colors';\n\
     const variants = {\n\
     primary: 'bg-blue-600 text-white hover:bg-blue-700',\n\
     secondary: 'bg-gray-200 text-gray-900 hover:bg-gray-300',\n\
     outline: 'border border-gray-300 hover:bg-gray-50',\n};\n\n\
     return (\n    <button\n\
     className={`${base} ${variants[variant]} ${className}`}\n\
     {...props}\n    >\n\
     {children}\n    </button>\n  );\n}\n"
    .into()
}

pub fn input_component() -> String {
    "import { InputHTMLAttributes } from 'react';\n\n\
     interface InputProps extends InputHTMLAttributes<HTMLInputElement> {\n\
     label?: string;\n\
     error?: string;\n}\n\n\
     export function Input({\n  label,\n  error,\n  className = '',\n  id,\n  ...props\n}: InputProps) {\n\
     const inputId = id || label?.toLowerCase().replace(/\\s+/g, '-');\n\n\
     return (\n    <div className=\"flex flex-col gap-1\">\n\
     {label && (\n      <label htmlFor={inputId} className=\"text-sm font-medium\">\n\
     {label}\n          </label>\n        )}\n\
     <input\n      id={inputId}\n\
     className={`px-3 py-2 border rounded-lg focus:outline-none \
     focus:ring-2 focus:ring-blue-500 ${error ? 'border-red-500' : 'border-gray-300'} ${className}`}\n\
     {...props}\n      />\n\
     {error && <p className=\"text-sm text-red-500\">{error}</p>}\n\
     </div>\n  );\n}\n"
    .into()
}

pub fn card() -> String {
    "import { HTMLAttributes } from 'react';\n\n\
     interface CardProps extends HTMLAttributes<HTMLDivElement> {\n\
     title?: string;\n\
     subtitle?: string;\n}\n\n\
     export function Card({\n  title,\n  subtitle,\n  children,\n  className = '',\n  ...props\n}: CardProps) {\n\
     return (\n    <div\n\
     className={`p-6 bg-white rounded-xl border ${className}`}\n\
     {...props}\n    >\n\
     {title && <h3 className=\"text-lg font-semibold mb-1\">{title}</h3>}\n\
     {subtitle && <p className=\"text-sm text-gray-500 mb-4\">{subtitle}</p>}\n\
     {children}\n    </div>\n  );\n}\n"
    .into()
}

pub fn header(ctx: &Ctx) -> String {
    format!(
        "import Link from 'next/link';\n\n\
         const links = [\n\
         {{ href: '/', label: 'Home' }},\n\
         {{ href: '/about', label: 'About' }},\n\
         {{ href: '/dashboard', label: 'Dashboard' }},\n];\n\n\
         export default function Header() {{\n\
         return (\n    <header className=\"bg-white border-b\">\n\
         <nav className=\"max-w-7xl mx-auto px-4 h-16 flex items-center gap-8\">\n\
         <span className=\"font-bold text-lg\">{name}</span>\n\
         <ul className=\"flex gap-6 ml-auto\">\n\
         {{links.map(({{ href, label }}) => (\n\
         <li key={{href}}>\n\
         <Link href={{href}}\n\
         className=\"text-gray-600 hover:text-gray-900 transition text-sm font-medium\">\n\
         {{label}}\n              </Link>\n            </li>\n\
         ))}}\n        </ul>\n      </nav>\n    </header>\n  );\n}}\n",
        name = ctx.name,
    )
}

pub fn footer(ctx: &Ctx) -> String {
    format!(
        "export default function Footer() {{\n\
         const year = new Date().getFullYear();\n\n\
         return (\n    <footer className=\"bg-gray-50 border-t mt-auto\">\n\
         <div className=\"max-w-7xl mx-auto px-4 py-8\">\n\
         <p className=\"text-center text-gray-500 text-sm\">\n\
         &copy; {{year}} {name}. All rights reserved.\n        </p>\n\
         </div>\n    </footer>\n  );\n}}\n",
        name = ctx.name,
    )
}

pub fn utils() -> String {
    "import { type ClassValue, clsx } from 'clsx';\n\
     import { twMerge } from 'tailwind-merge';\n\n\
     export function cn(...inputs: ClassValue[]) {\n\
     return twMerge(clsx(inputs));\n}\n\n\
     export function formatDate(date: Date): string {\n\
     return new Intl.DateTimeFormat('en-US', {\n\
     year: 'numeric',\n    month: 'long',\n    day: 'numeric',\n}).format(date);\n}\n"
    .into()
}

pub fn db_stub() -> String {
    "// Database client stub\n\
     // Replace with your preferred database driver\n\n\
     // Example: Neon serverless Postgres\n\
     // import { neon } from '@neondatabase/serverless';\n\
     // export const sql = neon(process.env.DATABASE_URL!);\n\n\
     export const db = {\n\
     async query(text: string, params?: unknown[]) {\n\
     console.log('DB query:', text, params);\n\
     return { rows: [] };\n },\n};\n"
    .into()
}
