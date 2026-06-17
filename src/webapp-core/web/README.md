# Web Assets Directory

This folder contains all static assets that are served directly to the browser, such as:

- HTML files
- CSS stylesheets
- Images, icons, fonts
- Compiled JavaScript bundles (e.g., from bundlers like Webpack, Vite, or Bun)

The contents are typically copied or served by the web server without additional server‑side processing.

## Role of `static/`

All files placed under `static/` are considered public assets. They are copied as‑is to the distribution folder during the build step and served from the `/static` URL path.

## Build Process

We use **Bun** (or **pnpm**) as the JavaScript runtime and bundler. The typical workflow is:

```bash
# Install dependencies (if any)
 bun install   # or pnpm install

# Build the assets
 bun run build   # runs the build script defined in package.json
```

The build script should compile TypeScript/JS sources, bundle CSS/JS into `static/bundle/`, and copy the result to the `dist/` directory.

## Development Server

During development you can serve the files locally with a simple static‑file server:

```bash
 bun run dev   # runs a dev server, e.g., using `bun serve` or `pnpm dev`
```

This will serve the files from `src/webapp-core/web` so you can open `http://localhost:3000` in the browser.

## Adding New Assets

Place new HTML, CSS, JS, or image files under `static/`. After running the build step they will be available under the `/static` URL path.
