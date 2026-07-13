import { loadConfig } from "../config/app.js";
import { healthRoute } from "../routes/health.js";

export function buildApp(app) {
  const config = loadConfig();

  app.get("/health", async (req, res) => {
    const result = await healthRoute();
    res.json(result);
  });
  app.get("/", (req, res) => {
    res.json({
      service: config.name,
      framework: config.framework,
      mode: "fullstack",
      message: "MegaGate fullstack API scaffold ready",
    });
  });

  return app;
}
