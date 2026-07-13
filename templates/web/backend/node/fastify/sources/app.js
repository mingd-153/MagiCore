import { loadConfig } from "../config/app.js";
import { healthRoute } from "../routes/health.js";

export function buildApp(app) {
  const config = loadConfig();

  app.get("/health", async () => healthRoute());
  app.get("/", async () => ({
    service: config.name,
    framework: config.framework,
    mode: "fullstack",
    message: "MegaGate fullstack API scaffold ready"
  }));

  return app;
}
