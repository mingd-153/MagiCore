import { serve } from "@hono/node-server";
import { buildApp } from "./lib/app.js";

const port = Number(process.env.PORT ?? 3000);
const app = buildApp();

serve({
  fetch: app.fetch,
  port,
  hostname: "0.0.0.0",
});

console.log(`Hono server running on http://0.0.0.0:${port}`);
