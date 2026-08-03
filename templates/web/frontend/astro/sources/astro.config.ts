import { defineConfig } from 'astro/config';
import path from 'path';
import { fileURLToPath } from 'url';

export default defineConfig({
  vite: {
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
  },
});
