import type { Express, Request, Response } from "express";
import { loadConfig } from "../config/app.js";
import { healthRoute } from "../routes/health.js";

export function buildApp(app: Express): Express {
  const config = loadConfig();

  app.get("/health", async (_req: Request, res: Response) => {
    const result = await healthRoute();
    res.json(result);
  });
  app.get("/", (_req: Request, res: Response) => {
    res.json({
      service: config.name,
      framework: config.framework,
      mode: "fullstack",
      message: "MegaGate fullstack API scaffold ready",
    });
  });

  return app;
}
