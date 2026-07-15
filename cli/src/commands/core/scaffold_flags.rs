use clap::Args;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Args, Serialize, Deserialize, Default)]
#[command(next_help_heading = "Core")]
pub struct ScaffoldFlags {
    // ── Project identity ──────────────────────────────────────────
    #[arg(long, help = "Project display name (defaults to directory name)")]
    pub name: Option<String>,
    #[arg(long, help = "Target directory")]
    pub dir: Option<String>,
    #[arg(long, help = "Package manager (npm|pnpm|yarn|bun)")]
    pub pm: Option<String>,
    #[arg(long, help = "Init git repo")]
    pub git: bool,
    #[arg(
        long,
        help = "Auto-install dependencies after scaffold",
        default_value_t = true
    )]
    pub install: bool,
    #[arg(short = 'y', long, help = "Skip prompts, use defaults")]
    pub yes: bool,
    #[arg(long, help = "Use a preset (t3, mern, jamstack, saas, ...)")]
    pub preset: Option<String>,

    // ── Project type ──────────────────────────────────────────────
    #[arg(
        long,
        help = "Project layout type (frontend|backend|fullstack|monorepo)"
    )]
    pub project_type: Option<String>,

    // ── Language ──────────────────────────────────────────────────
    #[arg(long, help = "Use TypeScript")]
    pub ts: bool,
    #[arg(long, help = "Use JavaScript")]
    pub js: bool,

    // ── Frontend framework (mutually exclusive) ───────────────────
    #[arg(long, help = "React (Vite)")]
    pub react: bool,
    #[arg(long, help = "Next.js")]
    pub next: bool,
    #[arg(long, help = "Vue 3")]
    pub vue: bool,
    #[arg(long, help = "Nuxt 3")]
    pub nuxt: bool,
    #[arg(long, help = "Svelte")]
    pub svelte: bool,
    #[arg(long, help = "SvelteKit")]
    pub sveltekit: bool,
    #[arg(long, help = "SolidJS")]
    pub solid: bool,
    #[arg(long, help = "Astro")]
    pub astro: bool,
    #[arg(long, help = "Remix")]
    pub remix: bool,
    #[arg(long, help = "Force Vite as bundler")]
    pub vite: bool,

    // ── Styling ───────────────────────────────────────────────────
    #[arg(long, visible_alias = "tailwind", help = "Tailwind CSS")]
    pub tailwindcss: bool,
    #[arg(long, help = "CSS Modules")]
    pub css_modules: bool,
    #[arg(long, help = "styled-components")]
    pub styled_components: bool,
    #[arg(long, help = "Sass/SCSS")]
    pub sass: bool,
    #[arg(long, help = "UnoCSS")]
    pub unocss: bool,
    #[arg(long, help = "shadcn/ui (implies --tailwindcss)")]
    pub shadcn: bool,
    #[arg(long, help = "DaisyUI (implies --tailwindcss)")]
    pub daisyui: bool,

    // ── State management ──────────────────────────────────────────
    #[arg(long, help = "Zustand")]
    pub zustand: bool,
    #[arg(long, help = "Redux Toolkit")]
    pub redux: bool,
    #[arg(long, help = "Jotai")]
    pub jotai: bool,
    #[arg(long, help = "Recoil")]
    pub recoil: bool,
    #[arg(long, help = "Pinia (requires --vue/--nuxt)")]
    pub pinia: bool,
    #[arg(long, help = "TanStack Query")]
    pub tanstack_query: bool,

    // ── Backend framework ─────────────────────────────────────────
    #[arg(long, help = "Express.js")]
    pub express: bool,
    #[arg(long, help = "Fastify")]
    pub fastify: bool,
    #[arg(long, help = "NestJS")]
    pub nestjs: bool,
    #[arg(long, help = "Hono")]
    pub hono: bool,
    #[arg(long, help = "Koa")]
    pub koa: bool,
    #[arg(long, help = "tRPC server")]
    pub trpc: bool,

    // ── Database / ORM ────────────────────────────────────────────
    #[arg(long, help = "Prisma ORM")]
    pub prisma: bool,
    #[arg(long, help = "Drizzle ORM")]
    pub drizzle: bool,
    #[arg(long, help = "TypeORM")]
    pub typeorm: bool,
    #[arg(long, help = "Mongoose (MongoDB)")]
    pub mongoose: bool,
    #[arg(long, help = "PostgreSQL")]
    pub postgres: bool,
    #[arg(long, help = "MySQL")]
    pub mysql: bool,
    #[arg(long, help = "SQLite")]
    pub sqlite: bool,
    #[arg(long, help = "MongoDB")]
    pub mongodb: bool,

    // ── Validation ────────────────────────────────────────────────
    #[arg(long, help = "Zod")]
    pub zod: bool,
    #[arg(long, help = "Yup")]
    pub yup: bool,
    #[arg(long, help = "Joi")]
    pub joi: bool,
    #[arg(long, help = "Valibot")]
    pub valibot: bool,

    // ── Authentication ────────────────────────────────────────────
    #[arg(long, help = "NextAuth/Auth.js")]
    pub nextauth: bool,
    #[arg(long, help = "Clerk")]
    pub clerk: bool,
    #[arg(long, help = "Lucia Auth")]
    pub lucia: bool,
    #[arg(long, help = "JWT auth")]
    pub jwt: bool,
    #[arg(long, help = "OAuth provider config")]
    pub oauth: bool,

    // ── Testing ───────────────────────────────────────────────────
    #[arg(long, help = "Vitest")]
    pub vitest: bool,
    #[arg(long, help = "Jest")]
    pub jest: bool,
    #[arg(long, help = "Playwright (E2E)")]
    pub playwright: bool,
    #[arg(long, help = "Cypress (E2E)")]
    pub cypress: bool,
    #[arg(long, help = "React/Vue Testing Library")]
    pub testing_library: bool,

    // ── Linting / Formatting ──────────────────────────────────────
    #[arg(long, help = "ESLint")]
    pub eslint: bool,
    #[arg(long, help = "Prettier")]
    pub prettier: bool,
    #[arg(long, help = "Biome (replaces ESLint+Prettier)")]
    pub biome: bool,
    #[arg(long, help = "Husky git hooks")]
    pub husky: bool,
    #[arg(long, help = "lint-staged")]
    pub lint_staged: bool,
    #[arg(long, help = "commitlint")]
    pub commitlint: bool,

    // ── Monorepo tooling ──────────────────────────────────────────
    #[arg(long, help = "Enable monorepo structure")]
    pub monorepo: bool,
    #[arg(long, help = "Turborepo")]
    pub turborepo: bool,
    #[arg(long, help = "Nx")]
    pub nx: bool,
    #[arg(long, help = "Native npm/pnpm/yarn workspaces")]
    pub workspaces: bool,
    #[arg(long, help = "Changesets for version/publish management")]
    pub changesets: bool,

    // ── API Layer ─────────────────────────────────────────────────
    #[arg(long, help = "REST API")]
    pub rest: bool,
    #[arg(long, help = "GraphQL")]
    pub graphql: bool,
    #[arg(long, help = "tRPC")]
    pub trpc_api: bool,
    #[arg(long, help = "gRPC")]
    pub grpc: bool,

    // ── Deployment / CI/CD ────────────────────────────────────────
    #[arg(long, help = "Generate Dockerfile + docker-compose")]
    pub docker: bool,
    #[arg(long, help = "GitHub Actions CI workflow")]
    pub github_actions: bool,
    #[arg(long, help = "Vercel deploy config")]
    pub vercel: bool,
    #[arg(long, help = "Railway deploy config")]
    pub railway: bool,
    #[arg(long, help = "Fly.io deploy config")]
    pub fly: bool,

    // ── Misc ──────────────────────────────────────────────────────
    #[arg(long, help = "Setup .env + .env.example")]
    pub dotenv: bool,
    #[arg(long, help = "Internationalization (i18n)")]
    pub i18n: bool,
    #[arg(long, help = "Progressive Web App")]
    pub pwa: bool,
    #[arg(long, help = "Storybook")]
    pub storybook: bool,
    #[arg(long, help = "Sentry error tracking")]
    pub sentry: bool,
    #[arg(long, help = "Analytics (Google/Plausible)")]
    pub analytics: bool,

    // ── Extra features ────────────────────────────────────────────
    #[arg(long = "feature", help = "Additional feature flag (repeatable)")]
    pub features: Vec<String>,
}
