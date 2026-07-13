import { Hono } from "hono";
import { loadConfig } from "../config/app.js";
import { healthRoute } from "../routes/health.js";

export function buildApp() {
  const config = loadConfig();
  const app = new Hono();

  app.get("/health", async (c) => c.json(await healthRoute()));
  app.get("/", (c) => c.json({
    service: config.name,
    framework: config.framework,
    mode: "fullstack",
    message: "MegaGate fullstack API scaffold ready",
  }));

  return app;
}