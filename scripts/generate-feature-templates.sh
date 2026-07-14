#!/usr/bin/env bash
set -euo pipefail

ROOT="/Users/doanmihh/Documents/Workspace/MegaGate"
RVS="$ROOT/templates/web/frontend/react-vite/sources"

SHARED_FILES=(
  "package.json" "biome.json" "eslintrc.json" "prettierrc"
  "components.json" "vercel.json" "babel.config.json"
  "tsconfig.json" "jsconfig.json" "vite-env.d.ts"
  "postcss.config.js" "postcss.config.mjs"
  "tailwind.config.ts" "tailwind.config.js"
  "uno.config.ts" "uno.config.js"
  "vite.config.ts" "vite.config.js"
  "vite.config.tailwind.ts" "vite.config.tailwind.js"
  "vitest.config.ts" "vitest.config.js"
  "jest.config.ts" "jest.config.js"
  "cypress.config.ts" "cypress.config.js"
  "playwright.config.ts" "playwright.config.js"
  "drizzle.config.ts" "drizzle.config.js"
  "sentry.client.config.ts" "sentry.client.config.js"
  "i18n.config.ts" "i18n.config.js"
)

# -- helpers --
mono_source() {
  local path="$1"
  case "$path" in
    src/app/*) echo "$path" ;;
    src/*) echo "${path#src/}" ;;
    *) echo "$path" ;;
  esac
}

write_template_toml() {
  local FW="$1" IS_MONO="$2"
  local PREFIX="" RELOC=""
  if [ "$IS_MONO" = "true" ]; then
    PREFIX="apps/frontend/"
    RELOC="reloc"
  fi

  local FW_SRC="$ROOT/templates/web/frontend/$FW/sources"
  local OUT="$ROOT/templates/web"
  [ "$IS_MONO" = "true" ] && OUT="$OUT/monorepo"
  OUT="$OUT/frontend/$FW/template.toml"

  # Map source paths — frontend keeps src/, monorepo strips for relocated dirs
  S() {
    local p="$1"
    if [ "$IS_MONO" = "true" ]; then
      mono_source "$p"
    else
      echo "$p"
    fi
  }

  write_entry() {
    local source_path="$1" target="$2"
    shift 2
    echo "[[files]]"
    echo "source = \"$(S "$source_path")\""
    echo "target = \"${PREFIX}${target}\""
    local mode="" inc=() exc=() req=()
    for arg in "$@"; do
      case "$arg" in
        --include) mode=include ;;
        --exclude) mode=exclude ;;
        --require) mode=require ;;
        *)
          if [ "$mode" = "include" ]; then inc+=("$arg")
          elif [ "$mode" = "exclude" ]; then exc+=("$arg")
          elif [ "$mode" = "require" ]; then req+=("$arg"); fi
          ;;
      esac
    done
    join_list() { local items=("$@"); local out=""; for it in "${items[@]}"; do [ -n "$out" ] && out="$out, "; out="${out}\"${it}\""; done; echo "$out"; }
    [ ${#inc[@]} -gt 0 ] && echo "include_features = [$(join_list "${inc[@]}")]"
    [ ${#exc[@]} -gt 0 ] && echo "exclude_features = [$(join_list "${exc[@]}")]"
    [ ${#req[@]} -gt 0 ] && echo "required_context = [$(join_list "${req[@]}")]"
    echo ""
  }

  # -- emit --
  {
    echo "# MegaGate template for $FW"
    echo "# Auto-generated — do not edit manually"
    echo ""

    if [ "$IS_MONO" = "true" ]; then
      write_entry "package.js.json" "package.json" --require project_slug --exclude typescript
      write_entry "package.ts.json" "package.json" --require project_slug --include typescript
    else
      write_entry "package.json" "package.json" --require project_slug
    fi

    # Framework-specific entries
    case "$FW" in
      sveltekit)
        write_entry "app.html" "src/app.html"
        write_entry "+layout.svelte" "src/routes/+layout.svelte"
        write_entry "+page.svelte" "src/routes/+page.svelte" --require project_name
        write_entry "app.css" "src/app.css"
        write_entry "svelte.config.js" "svelte.config.js" --exclude typescript
        write_entry "svelte.config.ts" "svelte.config.ts" --include typescript
        ;;
      solidjs)
        write_entry "index.ts.html" "index.html" --require project_name --include typescript
        write_entry "index.js.html" "index.html" --require project_name --exclude typescript
        write_entry "main.tsx" "src/main.tsx" --include typescript
        write_entry "main.jsx" "src/main.jsx" --exclude typescript
        write_entry "App.tsx" "src/App.tsx" --require project_name --include typescript
        write_entry "App.jsx" "src/App.jsx" --require project_name --exclude typescript
        write_entry "router/AppRouter.tsx" "src/router/AppRouter.tsx" --include typescript
        write_entry "router/AppRouter.jsx" "src/router/AppRouter.jsx" --exclude typescript
        ;;
      astro)
        write_entry "src/pages/index.astro" "src/pages/index.astro" --require project_name
        write_entry "src/layouts/Layout.astro" "src/layouts/Layout.astro"
        write_entry "astro.config.ts" "astro.config.ts" --include typescript
        write_entry "astro.config.mjs" "astro.config.mjs" --exclude typescript
        ;;
      qwik)
        write_entry "src/root.tsx" "src/root.tsx" --require project_name --include typescript
        write_entry "src/root.jsx" "src/root.jsx" --require project_name --exclude typescript
        write_entry "src/entry.ssr.tsx" "src/entry.ssr.tsx" --include typescript
        write_entry "src/entry.ssr.jsx" "src/entry.ssr.jsx" --exclude typescript
        write_entry "src/entry.dev.tsx" "src/entry.dev.tsx" --include typescript
        write_entry "src/entry.dev.jsx" "src/entry.dev.jsx" --exclude typescript
        write_entry "src/routes/index.tsx" "src/routes/index.tsx" --require project_name --include typescript
        write_entry "src/routes/index.jsx" "src/routes/index.jsx" --require project_name --exclude typescript
        ;;
      angular)
        write_entry "angular.json" "angular.json" --require project_slug
        write_entry "src/index.html" "src/index.html" --require project_name
        write_entry "src/main.ts" "src/main.ts"
        write_entry "src/styles.css" "src/styles.css"
        write_entry "src/app/app.config.ts" "src/app/app.config.ts"
        write_entry "src/app/app.routes.ts" "src/app/app.routes.ts"
        write_entry "src/app/app.component.ts" "src/app/app.component.ts" --require project_name
        write_entry "tsconfig.app.json" "tsconfig.app.json"
        ;;
      vanilla)
        write_entry "index.ts.html" "index.html" --require project_name --include typescript
        write_entry "index.js.html" "index.html" --require project_name --exclude typescript
        write_entry "main.ts" "src/main.ts" --include typescript
        write_entry "main.js" "src/main.js" --exclude typescript
        write_entry "App.ts" "src/App.ts" --require project_name --include typescript
        write_entry "App.js" "src/App.js" --require project_name --exclude typescript
        write_entry "router/AppRouter.ts" "src/router/AppRouter.ts" --include typescript
        write_entry "router/AppRouter.js" "src/router/AppRouter.js" --exclude typescript
        ;;
      nuxt)
        write_entry "nuxt.config.ts" "nuxt.config.ts" --include typescript
        write_entry "nuxt.config.js" "nuxt.config.js" --exclude typescript
        write_entry "app.vue" "app.vue"
        write_entry "pages/index.vue" "pages/index.vue" --require project_name
        ;;
    esac

    # Shared across all frameworks
    write_entry "jsconfig.json" "jsconfig.json" --exclude typescript
    write_entry "tsconfig.json" "tsconfig.json" --include typescript

    # vite-env + vite.config (for vite-based FWs)
    case "$FW" in
      solidjs|vanilla|sveltekit)
        write_entry "vite-env.d.ts" "src/vite-env.d.ts" --include typescript
        write_entry "vite.config.js" "vite.config.js" --exclude typescript tailwindcss
        write_entry "vite.config.ts" "vite.config.ts" --include typescript --exclude tailwindcss
        write_entry "vite.config.tailwind.js" "vite.config.js" --exclude typescript --include tailwindcss
        write_entry "vite.config.tailwind.ts" "vite.config.ts" --include tailwindcss
        ;;
      qwik)
        write_entry "vite.config.js" "vite.config.js" --exclude typescript
        write_entry "vite.config.ts" "vite.config.ts" --include typescript
        ;;
    esac

    # config/framework
    write_entry "config/framework.ts" "src/config/framework.ts" --include typescript
    write_entry "config/framework.js" "src/config/framework.js" --exclude typescript

    # tailwindcss + postcss
    write_entry "tailwind.config.js" "tailwind.config.js" --exclude typescript --include tailwindcss daisyui
    write_entry "tailwind.config.ts" "tailwind.config.ts" --include tailwindcss daisyui
    write_entry "postcss.config.js" "postcss.config.js" --exclude typescript --include tailwindcss
    write_entry "postcss.config.mjs" "postcss.config.mjs" --include tailwindcss

    # eslint / prettier / biome
    write_entry "eslintrc.json" ".eslintrc.json" --include eslint
    write_entry "prettierrc" ".prettierrc" --include prettier
    write_entry "biome.json" "biome.json" --include biome

    # vitest / jest
    write_entry "vitest.config.js" "vitest.config.js" --exclude typescript --include vitest
    write_entry "vitest.config.ts" "vitest.config.ts" --include vitest
    write_entry "jest.config.js" "jest.config.js" --exclude typescript --include jest
    write_entry "jest.config.ts" "jest.config.ts" --include jest

    # cypress / playwright
    write_entry "cypress.config.js" "cypress.config.js" --exclude typescript --include cypress
    write_entry "cypress.config.ts" "cypress.config.ts" --include cypress
    write_entry "playwright.config.js" "playwright.config.js" --exclude typescript --include playwright
    write_entry "playwright.config.ts" "playwright.config.ts" --include playwright
    write_entry "e2e/example.spec.ts" "e2e/example.spec.ts" --include playwright

    # zustand
    write_entry "src/store/zustand.js" "src/store/index.js" --exclude typescript --include zustand
    write_entry "src/store/zustand.ts" "src/store/index.ts" --include zustand

    # tanstack-query
    if [ "$FW" = "solidjs" ] || [ "$FW" = "qwik" ]; then
      write_entry "src/lib/query-provider.jsx" "src/lib/query-provider.jsx" --exclude typescript --include tanstack-query
      write_entry "src/lib/query-provider.tsx" "src/lib/query-provider.tsx" --include tanstack-query
    else
      write_entry "src/lib/query-provider.js" "src/lib/query-provider.js" --exclude typescript --include tanstack-query
      write_entry "src/lib/query-provider.ts" "src/lib/query-provider.ts" --include tanstack-query
    fi

    # zod
    write_entry "src/lib/schemas.js" "src/lib/schemas.js" --exclude typescript --include zod
    write_entry "src/lib/schemas.ts" "src/lib/schemas.ts" --include zod

    # shadcn
    write_entry "components.json" "components.json" --include shadcn
    write_entry "src/lib/utils.js" "src/lib/utils.js" --exclude typescript --include shadcn
    write_entry "src/lib/utils.ts" "src/lib/utils.ts" --include shadcn

    # redux
    write_entry "src/store/redux/index.js" "src/store/redux/index.js" --exclude typescript --include redux
    write_entry "src/store/redux/index.ts" "src/store/redux/index.ts" --include redux
    write_entry "src/store/redux/slices/counterSlice.js" "src/store/redux/slices/counterSlice.js" --exclude typescript --include redux
    write_entry "src/store/redux/slices/counterSlice.ts" "src/store/redux/slices/counterSlice.ts" --include redux

    # drizzle
    write_entry "drizzle.config.js" "drizzle.config.js" --exclude typescript --include drizzle
    write_entry "drizzle.config.ts" "drizzle.config.ts" --include drizzle
    write_entry "src/db/schema.js" "src/db/schema.js" --exclude typescript --include drizzle
    write_entry "src/db/schema.ts" "src/db/schema.ts" --include drizzle

    # prisma
    write_entry "prisma/schema.prisma" "prisma/schema.prisma" --include prisma

    # clerk
    write_entry "src/lib/clerk.js" "src/lib/clerk.js" --exclude typescript --include clerk
    write_entry "src/lib/clerk.ts" "src/lib/clerk.ts" --include clerk

    # vercel
    write_entry "vercel.json" "vercel.json" --include vercel

    # unocss
    write_entry "uno.config.js" "uno.config.js" --exclude typescript --include unocss
    write_entry "uno.config.ts" "uno.config.ts" --include unocss

    # sentry
    write_entry "sentry.client.config.js" "sentry.client.config.js" --exclude typescript --include sentry
    write_entry "sentry.client.config.ts" "sentry.client.config.ts" --include sentry

    # i18n
    write_entry "i18n.config.js" "i18n.config.js" --exclude typescript --include i18n
    write_entry "i18n.config.ts" "i18n.config.ts" --include i18n

    # pwa
    write_entry "public/manifest.json" "public/manifest.json" --include pwa
    write_entry "public/sw.js" "public/sw.js" --include pwa

    # grpc
    write_entry "grpc/greeter.proto" "grpc/greeter.proto" --include grpc

    # lucia/jwt/oauth
    write_entry "src/lib/auth.js" "src/lib/auth.js" --exclude typescript --include lucia jwt oauth
    write_entry "src/lib/auth.ts" "src/lib/auth.ts" --include lucia jwt oauth

    # styled-components
    if [ "$FW" = "solidjs" ] || [ "$FW" = "qwik" ]; then
      write_entry "src/components/StyledExample.jsx" "src/components/StyledExample.jsx" --exclude typescript --include styled-components
      write_entry "src/components/StyledExample.tsx" "src/components/StyledExample.tsx" --include styled-components
    else
      write_entry "src/components/StyledExample.js" "src/components/StyledExample.js" --exclude typescript --include styled-components
      write_entry "src/components/StyledExample.ts" "src/components/StyledExample.ts" --include styled-components
    fi
    write_entry "babel.config.json" "babel.config.json" --include styled-components

    # sass
    write_entry "src/styles/breakpoints.scss" "src/styles/breakpoints.scss" --include sass

    # globals.css (always)
    write_entry "src/styles/globals.css" "src/styles/globals.css"

    # middleware (clerk)
    if [ -f "$FW_SRC/src/middleware.ts" ]; then
      write_entry "src/middleware.ts" "src/middleware.ts" --include clerk
      write_entry "src/middleware.js" "src/middleware.js" --exclude typescript --include clerk
    fi

    # trpc / graphql
    write_entry "src/trpc/router.ts" "src/trpc/router.ts" --include trpc
    write_entry "src/lib/graphql/schema.ts" "src/lib/graphql/schema.ts" --include graphql

    # storybook
    write_entry ".storybook/main.js" ".storybook/main.js" --exclude typescript --include storybook
    write_entry ".storybook/main.ts" ".storybook/main.ts" --include storybook

    # pages/Home
    if [ "$FW" = "solidjs" ] || [ "$FW" = "qwik" ]; then
      write_entry "src/pages/Home.tsx" "src/pages/Home.tsx" --include typescript
      write_entry "src/pages/Home.jsx" "src/pages/Home.jsx" --exclude typescript
    else
      write_entry "src/pages/Home.ts" "src/pages/Home.ts" --include typescript
      write_entry "src/pages/Home.js" "src/pages/Home.js" --exclude typescript
    fi

    # REST API route
    write_entry "src/app/api/rest/route.ts" "src/app/api/rest/route.ts" --include rest

    # test setup
    write_entry "src/test/setup.ts" "src/test/setup.ts" --include vitest jest

  } > "$OUT"

  echo "  -> Wrote $OUT"
}

process_framework() {
  local FW="$1"
  local FW_DIR="$ROOT/templates/web/frontend/$FW"
  local FW_SRC="$FW_DIR/sources"
  local MONO_DIR="$ROOT/templates/web/monorepo/frontend/$FW"
  local MONO_SRC="$MONO_DIR/sources"

  echo "=== Processing framework: $FW ==="

  # Determine framework-specific settings
  local JS_EXT=".js"
  local TS_EXT=".ts"
  local USE_JSX=false

  case "$FW" in
    solidjs|qwik) JS_EXT=".jsx"; TS_EXT=".tsx"; USE_JSX=true ;;
  esac

  # Create dirs
  mkdir -p "$FW_SRC/config" "$FW_SRC/src/lib/graphql" "$FW_SRC/src/store/redux/slices"
  mkdir -p "$FW_SRC/src/db" "$FW_SRC/src/styles" "$FW_SRC/src/trpc"
  mkdir -p "$FW_SRC/src/test" "$FW_SRC/src/pages" "$FW_SRC/src/components"
  mkdir -p "$FW_SRC/src/app/api/rest" "$FW_SRC/src/app/api/graphql" "$FW_SRC/src/app/api/trpc"
  mkdir -p "$FW_SRC/grpc" "$FW_SRC/prisma" "$FW_SRC/public" "$FW_SRC/e2e" "$FW_SRC/.storybook"

  # Copy shared config files
  for f in "${SHARED_FILES[@]}"; do
    [ -f "$RVS/$f" ] && cp "$RVS/$f" "$FW_SRC/$f"
  done

  # Copy shared src files
  for d in src/lib src/store src/db src/styles src/trpc src/test grpc prisma public e2e .storybook src/app/api src/middleware.ts src/middleware.js; do
    [ -e "$RVS/$d" ] && cp -R "$RVS/$d" "$FW_SRC/$d" 2>/dev/null || true
  done

  # Copy pages/Home and components/StyledExample with correct extensions
  if [ "$USE_JSX" = true ]; then
    cp "$RVS/src/pages/Home.tsx" "$FW_SRC/src/pages/Home.tsx" 2>/dev/null || true
    cp "$RVS/src/pages/Home.jsx" "$FW_SRC/src/pages/Home.jsx" 2>/dev/null || true
    cp "$RVS/src/components/StyledExample.tsx" "$FW_SRC/src/components/StyledExample.tsx" 2>/dev/null || true
    cp "$RVS/src/components/StyledExample.jsx" "$FW_SRC/src/components/StyledExample.jsx" 2>/dev/null || true
    cp "$RVS/src/lib/query-provider.tsx" "$FW_SRC/src/lib/query-provider.tsx" 2>/dev/null || true
    cp "$RVS/src/lib/query-provider.jsx" "$FW_SRC/src/lib/query-provider.jsx" 2>/dev/null || true
  else
    cp "$RVS/src/pages/Home.tsx" "$FW_SRC/src/pages/Home.ts" 2>/dev/null || true
    cp "$RVS/src/pages/Home.jsx" "$FW_SRC/src/pages/Home.js" 2>/dev/null || true
    cp "$RVS/src/components/StyledExample.tsx" "$FW_SRC/src/components/StyledExample.ts" 2>/dev/null || true
    cp "$RVS/src/components/StyledExample.jsx" "$FW_SRC/src/components/StyledExample.js" 2>/dev/null || true
    cp "$RVS/src/lib/query-provider.tsx" "$FW_SRC/src/lib/query-provider.ts" 2>/dev/null || true
    cp "$RVS/src/lib/query-provider.jsx" "$FW_SRC/src/lib/query-provider.js" 2>/dev/null || true
  fi

  # -- Framework-specific files --

  # config/framework.{js,ts}
  case "$FW" in
    solidjs)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Solid",
  docs: { label: "Explore Solid", href: "https://solidjs.com" },
  signal: ["Solid-first", "Fine-grained", "Powered by mg"],
};
ENDCONF
      cat > "$FW_SRC/config/framework.js" << 'ENDCONF'
export const frameworkConfig = {
  shortName: "Solid",
  docs: { label: "Explore Solid", href: "https://solidjs.com" },
  signal: ["Solid-first", "Fine-grained", "Powered by mg"],
};
ENDCONF
      ;;
    qwik)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Qwik",
  docs: { label: "Explore Qwik", href: "https://qwik.dev" },
  signal: ["Resumable", "Instant", "Powered by mg"],
};
ENDCONF
      cat > "$FW_SRC/config/framework.js" << 'ENDCONF'
export const frameworkConfig = {
  shortName: "Qwik",
  docs: { label: "Explore Qwik", href: "https://qwik.dev" },
  signal: ["Resumable", "Instant", "Powered by mg"],
};
ENDCONF
      ;;
    angular)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Angular",
  docs: { label: "Explore Angular", href: "https://angular.dev" },
  signal: ["Enterprise-grade", "Modular", "Powered by mg"],
};
ENDCONF
      ;;
    vanilla)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Vanilla",
  docs: { label: "Explore Web APIs", href: "https://developer.mozilla.org" },
  signal: ["Lightweight", "Zero-deps", "Powered by mg"],
};
ENDCONF
      cat > "$FW_SRC/config/framework.js" << 'ENDCONF'
export const frameworkConfig = {
  shortName: "Vanilla",
  docs: { label: "Explore Web APIs", href: "https://developer.mozilla.org" },
  signal: ["Lightweight", "Zero-deps", "Powered by mg"],
};
ENDCONF
      ;;
    astro)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Astro",
  docs: { label: "Explore Astro", href: "https://astro.build" },
  signal: ["Content-first", "Islands", "Powered by mg"],
};
ENDCONF
      cat > "$FW_SRC/config/framework.js" << 'ENDCONF'
export const frameworkConfig = {
  shortName: "Astro",
  docs: { label: "Explore Astro", href: "https://astro.build" },
  signal: ["Content-first", "Islands", "Powered by mg"],
};
ENDCONF
      ;;
    nuxt)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Nuxt",
  docs: { label: "Explore Nuxt", href: "https://nuxt.com" },
  signal: ["Vue-first", "SSR-ready", "Powered by mg"],
};
ENDCONF
      cat > "$FW_SRC/config/framework.js" << 'ENDCONF'
export const frameworkConfig = {
  shortName: "Nuxt",
  docs: { label: "Explore Nuxt", href: "https://nuxt.com" },
  signal: ["Vue-first", "SSR-ready", "Powered by mg"],
};
ENDCONF
      ;;
    sveltekit)
      cat > "$FW_SRC/config/framework.ts" << 'ENDCONF'
interface ProjectLink { label: string; url: string; }
export const projectLinks: ProjectLink[] = [
  { label: "SvelteKit docs", url: "https://svelte.dev/docs/kit" },
  { label: "MegaGate docs", url: "https://megagate.dev/docs" },
];
ENDCONF
      cat > "$FW_SRC/config/framework.js" << 'ENDCONF'
export const projectLinks = [
  { label: "SvelteKit docs", url: "https://svelte.dev/docs/kit" },
  { label: "MegaGate docs", url: "https://megagate.dev/docs" },
];
ENDCONF
      ;;
  esac

  # SolidJS entry points (index.html, main, App, Router)
  if [ "$FW" = "solidjs" ]; then
    mkdir -p "$FW_SRC/router"
    cat > "$FW_SRC/index.ts.html" << 'ENDI'
<!doctype html><html lang="en"><head><meta charset="UTF-8"/><meta name="viewport" content="width=device-width,initial-scale=1.0"/><link rel="icon" type="image/x-icon" href="/favicon.ico"/><title>{{project_name}}</title></head><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>
ENDI
    cat > "$FW_SRC/index.js.html" << 'ENDI'
<!doctype html><html lang="en"><head><meta charset="UTF-8"/><meta name="viewport" content="width=device-width,initial-scale=1.0"/><link rel="icon" type="image/x-icon" href="/favicon.ico"/><title>{{project_name}}</title></head><body><div id="root"></div><script type="module" src="/src/main.jsx"></script></body></html>
ENDI
    cat > "$FW_SRC/main.tsx" << 'ENDM'
import { render } from "solid-js/web";
import "./styles/globals.css";
import { App } from "./App";
render(() => <App />, document.getElementById("root")!);
ENDM
    cat > "$FW_SRC/main.jsx" << 'ENDM'
import { render } from "solid-js/web";
import "./styles/globals.css";
import { App } from "./App";
render(() => <App />, document.getElementById("root"));
ENDM
    cat > "$FW_SRC/App.tsx" << 'ENDA'
import { AppRouter } from "./router/AppRouter";
export function App() { return <AppRouter />; }
ENDA
    cat > "$FW_SRC/App.jsx" << 'ENDA'
import { AppRouter } from "./router/AppRouter";
export function App() { return <AppRouter />; }
ENDA
    cat > "$FW_SRC/router/AppRouter.tsx" << 'ENDR'
export function AppRouter() { return <main>Router placeholder</main>; }
ENDR
    cat > "$FW_SRC/router/AppRouter.jsx" << 'ENDR'
export function AppRouter() { return <main>Router placeholder</main>; }
ENDR
  fi

  # Vanilla entry points
  if [ "$FW" = "vanilla" ]; then
    mkdir -p "$FW_SRC/router"
    cat > "$FW_SRC/index.ts.html" << 'ENDI'
<!doctype html><html lang="en"><head><meta charset="UTF-8"/><meta name="viewport" content="width=device-width,initial-scale=1.0"/><link rel="icon" type="image/x-icon" href="/favicon.ico"/><title>{{project_name}}</title></head><body><div id="root"></div><script type="module" src="/src/main.ts"></script></body></html>
ENDI
    cat > "$FW_SRC/index.js.html" << 'ENDI'
<!doctype html><html lang="en"><head><meta charset="UTF-8"/><meta name="viewport" content="width=device-width,initial-scale=1.0"/><link rel="icon" type="image/x-icon" href="/favicon.ico"/><title>{{project_name}}</title></head><body><div id="root"></div><script type="module" src="/src/main.js"></script></body></html>
ENDI
    cat > "$FW_SRC/main.ts" << 'ENDM'
import "./styles/globals.css";
import { App } from "./App";
document.addEventListener("DOMContentLoaded", () => {
  const root = document.getElementById("root");
  if (root) App(root);
});
ENDM
    cat > "$FW_SRC/main.js" << 'ENDM'
import "./styles/globals.css";
import { App } from "./App";
document.addEventListener("DOMContentLoaded", () => {
  const root = document.getElementById("root");
  if (root) App(root);
});
ENDM
    cat > "$FW_SRC/App.ts" << 'ENDA'
import { AppRouter } from "./router/AppRouter";
export function App(root: HTMLElement) {
  const h1 = document.createElement("h1");
  h1.textContent = "{{project_name}}";
  root.appendChild(h1);
  AppRouter(root);
}
ENDA
    cat > "$FW_SRC/App.js" << 'ENDA'
import { AppRouter } from "./router/AppRouter";
export function App(root) {
  const h1 = document.createElement("h1");
  h1.textContent = "{{project_name}}";
  root.appendChild(h1);
  AppRouter(root);
}
ENDA
    cat > "$FW_SRC/router/AppRouter.ts" << 'ENDR'
export function AppRouter(root: HTMLElement) {
  const p = document.createElement("p");
  p.textContent = "Router placeholder";
  root.appendChild(p);
}
ENDR
    cat > "$FW_SRC/router/AppRouter.js" << 'ENDR'
export function AppRouter(root) {
  const p = document.createElement("p");
  p.textContent = "Router placeholder";
  root.appendChild(p);
}
ENDR
  fi

  # Qwik entry files
  if [ "$FW" = "qwik" ]; then
    mkdir -p "$FW_SRC/src/routes" "$FW_SRC/src/components"
    cat > "$FW_SRC/src/root.tsx" << 'ENDQ'
import { component$ } from '@builder.io/qwik';
import { QwikCityProvider, RouterOutlet, ServiceWorkerRegister } from '@builder.io/qwik-city';
export default component$(() => (
  <QwikCityProvider>
    <head><meta charSet="utf-8"/><title>{{project_name}}</title></head>
    <body lang="en"><RouterOutlet /><ServiceWorkerRegister /></body>
  </QwikCityProvider>
));
ENDQ
    cat > "$FW_SRC/src/root.jsx" << 'ENDQ'
import { component$ } from '@builder.io/qwik';
import { QwikCityProvider, RouterOutlet, ServiceWorkerRegister } from '@builder.io/qwik-city';
export default component$(() => (
  <QwikCityProvider>
    <head><meta charSet="utf-8"/><title>{{project_name}}</title></head>
    <body lang="en"><RouterOutlet /><ServiceWorkerRegister /></body>
  </QwikCityProvider>
));
ENDQ
    cat > "$FW_SRC/src/entry.ssr.tsx" << 'ENDQ'
import { renderToStream, RenderToStreamOptions } from '@builder.io/qwik/server';
import { manifest } from '@qwik-client-manifest';
import Root from './root';
export default function (opts: RenderToStreamOptions) {
  return renderToStream(<Root />, { manifest, ...opts, containerAttributes: { lang: 'en' } });
}
ENDQ
    cat > "$FW_SRC/src/entry.ssr.jsx" << 'ENDQ'
import { renderToStream, RenderToStreamOptions } from '@builder.io/qwik/server';
import { manifest } from '@qwik-client-manifest';
import Root from './root';
export default function (opts: RenderToStreamOptions) {
  return renderToStream(<Root />, { manifest, ...opts, containerAttributes: { lang: 'en' } });
}
ENDQ
    cat > "$FW_SRC/src/entry.dev.tsx" << 'ENDQ'
import { renderDev, RenderOptions } from '@builder.io/qwik/server';
import Root from './root';
export default function (opts: RenderOptions) { return renderDev(<Root />, opts); }
ENDQ
    cat > "$FW_SRC/src/entry.dev.jsx" << 'ENDQ'
import { renderDev, RenderOptions } from '@builder.io/qwik/server';
import Root from './root';
export default function (opts: RenderOptions) { return renderDev(<Root />, opts); }
ENDQ
    cat > "$FW_SRC/src/routes/index.tsx" << 'ENDQ'
import { component$ } from '@builder.io/qwik';
export default component$(() => (<div><h1>{{project_name}}</h1><p>Scaffolded with MegaGate · Qwik</p></div>));
ENDQ
    cat > "$FW_SRC/src/routes/index.jsx" << 'ENDQ'
import { component$ } from '@builder.io/qwik';
export default component$(() => (<div><h1>{{project_name}}</h1><p>Scaffolded with MegaGate · Qwik</p></div>));
ENDQ
  fi

  # Astro entry files
  if [ "$FW" = "astro" ]; then
    mkdir -p "$FW_SRC/src/pages" "$FW_SRC/src/layouts"
    cat > "$FW_SRC/astro.config.ts" << 'ENDA'
import { defineConfig } from 'astro/config';
export default defineConfig({});
ENDA
    cat > "$FW_SRC/astro.config.mjs" << 'ENDA'
import { defineConfig } from 'astro/config';
export default defineConfig({});
ENDA
    cat > "$FW_SRC/src/pages/index.astro" << 'ENDA'
---
import Layout from '../layouts/Layout.astro';
const projectName = '{{project_name}}';
---
<Layout title={projectName}>
  <main><h1>{projectName}</h1><p>Scaffolded with MegaGate · Astro</p></main>
</Layout>
ENDA
    cat > "$FW_SRC/src/layouts/Layout.astro" << 'ENDA'
---
const { title } = Astro.props;
---
<!doctype html><html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/><title>{title}</title></head><body><slot /></body></html>
ENDA
  fi

  # --- Generate template.toml ---
  write_template_toml "$FW" false

  # ============================================================
  #  Monorepo variant
  # ============================================================
  echo "  -> Creating monorepo variant"
  mkdir -p "$MONO_SRC"

  # Clean monorepo sources (keep .gitkeep)
  for item in "$MONO_SRC"/* "$MONO_SRC"/.*; do
    bname="$(basename "$item" 2>/dev/null || true)"
    case "$bname" in .|..) continue ;; esac
    [ ! -e "$item" ] && [ ! -L "$item" ] && continue
    [ "$bname" = ".gitkeep" ] && continue
    rm -rf "$item" 2>/dev/null || true
  done

  # Symlink all top-level items from frontend sources
  for item in "$FW_SRC"/*; do
    bname="$(basename "$item")"
    [ "$bname" = "package.json" ] && continue
    ln -sfn "../../../../frontend/$FW/sources/$bname" "$MONO_SRC/$bname"
  done
  # Dotfiles
  for item in "$FW_SRC"/.*; do
    bname="$(basename "$item")"
    case "$bname" in .|..|.gitkeep|.github) continue ;; esac
    ln -sfn "../../../../frontend/$FW/sources/$bname" "$MONO_SRC/$bname"
  done

  # Top-level symlinks for relocated subdirectories
  if [ "$FW" != "angular" ] && [ "$FW" != "astro" ] && [ "$FW" != "sveltekit" ] && [ "$FW" != "nuxt" ]; then
    for subdir in lib store db styles trpc pages components test; do
      [ -d "$FW_SRC/src/$subdir" ] && ln -sfn "../../../../frontend/$FW/sources/src/$subdir" "$MONO_SRC/$subdir"
    done
    # middleware files
    for m in middleware.ts middleware.js; do
      [ -f "$FW_SRC/src/$m" ] && ln -sfn "../../../../frontend/$FW/sources/src/$m" "$MONO_SRC/$m"
    done
  else
    # For these frameworks, symlink src/ directly (they have framework-specific src content)
    ln -sfn "../../../../frontend/$FW/sources/src" "$MONO_SRC/src"
  fi

  # Monorepo package files
  cat > "$MONO_SRC/package.js.json" << 'PKG'
{
  "name": "{{project_slug}}-frontend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": { "dev": "vite", "build": "vite build", "preview": "vite preview" }
}
PKG
  cp "$MONO_SRC/package.js.json" "$MONO_SRC/package.ts.json"

  # Generate monorepo template.toml
  write_template_toml "$FW" true

  echo "=== Done: $FW ==="
}

# ============================================================
FRAMEWORKS=("$@")
if [ ${#FRAMEWORKS[@]} -eq 0 ]; then
  FRAMEWORKS=("sveltekit" "solidjs" "astro" "qwik" "angular" "vanilla" "nuxt")
fi

echo "Generating feature templates for: ${FRAMEWORKS[*]}"
for fw in "${FRAMEWORKS[@]}"; do
  process_framework "$fw"
done
echo "All done!"
