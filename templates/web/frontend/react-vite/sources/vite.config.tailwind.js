import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import os from "os";
import path from "path";

function resolveMgCache() {
  if (process.env.MEGAGATE_SHARED_CACHE_DIR) return process.env.MEGAGATE_SHARED_CACHE_DIR;
  if (process.platform === "darwin") return path.join(os.homedir(), "Library", "Caches", "megagate");
  if (process.platform === "win32") return path.join(os.homedir(), "AppData", "Local", "megagate");
  return path.join(os.homedir(), ".cache", "megagate");
}

const mgCache = resolveMgCache();
const fsAllow = [process.cwd()];
if (mgCache) fsAllow.push(mgCache);

export default defineConfig({
  plugins: [tailwindcss(), react()],
  optimizeDeps: { include: ["react-dom/client"] },
  server: { fs: { allow: fsAllow } },
});