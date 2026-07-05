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
  "type": "module",
  "scripts": {{
    "dev": "tsx watch src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js",
    "lint": "eslint .",
    "format": "prettier --write ."
  }},
  "dependencies": {{
    "fastify": "{FASTIFY}",
    "@fastify/cors": "{FASTIFY_CORS}",
    "@fastify/helmet": "{FASTIFY_HELMET}",
    "zod": "{ZOD}"
  }},
  "devDependencies": {{
    "@types/node": "{TYPES_NODE}",
    "typescript": "{TYPESCRIPT}",
    "tsx": "{TSX}",
    "eslint": "{ESLINT}",
    "@eslint/js": "{ESLINT_JS}",
    "globals": "{GLOBALS}",
    "typescript-eslint": "{TYPESCRIPT_ESLINT}",
    "prettier": "{PRETTIER}"
  }}
}}"#,
        name = ctx.name,
        version = ctx.version,
        FASTIFY = FASTIFY(),
        FASTIFY_CORS = FASTIFY_CORS(),
        FASTIFY_HELMET = FASTIFY_HELMET(),
        ZOD = ZOD(),
        TYPES_NODE = TYPES_NODE(),
        TYPESCRIPT = TYPESCRIPT(),
        TSX = TSX(),
        ESLINT = ESLINT(),
        ESLINT_JS = ESLINT_JS(),
        GLOBALS = GLOBALS(),
        TYPESCRIPT_ESLINT = TYPESCRIPT_ESLINT(),
        PRETTIER = PRETTIER(),
    )
}

pub fn tsconfig_json() -> &'static str {
    r#"{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "declaration": true,
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}"#
}

pub fn index_ts() -> &'static str {
    r#"import { buildApp } from './app';

async function main() {
  const app = await buildApp();
  const port = parseInt(process.env.PORT || '3000', 10);
  await app.listen({ port });
  console.log(`Server running on http://localhost:${port}`);
}

main();"#
}

pub fn app_ts() -> &'static str {
    r#"import Fastify from 'fastify';
import cors from '@fastify/cors';
import { itemsRouter } from './routes/items';

export async function buildApp() {
  const app = Fastify({ logger: true });

  await app.register(cors);

  app.get('/health', async () => ({ status: 'ok' }));

  await app.register(itemsRouter, { prefix: '/api/items' });

  return app;
}"#
}

pub fn items_ts() -> &'static str {
    r#"import { FastifyPluginAsync } from 'fastify';
import { z } from 'zod';

interface Item {
  id: string;
  name: string;
  description?: string;
}

const items: Item[] = [];

const createItemSchema = z.object({
  name: z.string().min(1),
  description: z.string().optional(),
});

const itemsRouter: FastifyPluginAsync = async (fastify) => {
  fastify.get('/', async () => ({ data: items }));

  fastify.get<{ Params: { id: string } }>('/:id', async (request, reply) => {
    const item = items.find((i) => i.id === request.params.id);
    if (!item) {
      reply.code(404);
      return { message: 'Item not found' };
    }
    return { data: item };
  });

  fastify.post('/', async (request, reply) => {
    const parsed = createItemSchema.safeParse(request.body);
    if (!parsed.success) {
      reply.code(400);
      return { message: 'Validation failed', errors: parsed.error.flatten() };
    }
    const item: Item = { id: String(items.length + 1), ...parsed.data };
    items.push(item);
    reply.code(201);
    return { data: item };
  });
};

export { itemsRouter };"#
}

pub fn types_index_ts() -> &'static str {
    r#"export interface ApiResponse<T> {
  data: T;
  message?: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  pageSize: number;
}"#
}

pub fn dockerfile() -> &'static str {
    r#"FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json ./
RUN npm install
COPY . .
RUN npm run build

FROM node:22-alpine AS runner
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY package.json ./
EXPOSE 3000
CMD ["node", "dist/index.js"]"#
}

pub fn gitignore() -> &'static str {
    r#"node_modules/
dist/
.env
.env.local
*.log
.DS_Store"#
}

pub fn env_example() -> &'static str {
    r#"PORT=3000"#
}

pub fn readme(ctx: &Ctx) -> String {
    format!(
        r#"# {name}

Fastify REST API built with TypeScript.

## Commands

```bash
npm run dev      # Start dev server
npm run build    # Build for production
npm start        # Start production server
```
"#,
        name = ctx.name,
    )
}
