import express from "express";
import { createContext } from "../context.js";
import { appRouter } from "../router.js";
import { loadConfig } from "../config/app.js";
import * as trpcExpress from "@trpc/server/adapters/express";

export function buildApp(): express.Express {
  const config = loadConfig();
  const app = express();

  app.use("/trpc", trpcExpress.createExpressMiddleware({
    router: appRouter,
    createContext,
  }));

  app.get("/health", async (_req, res) => {
    res.json(await import("../routes/health.js").then(m => m.healthRoute()));
  });
  app.get("/", (_req, res) => {
    res.json({
      service: config.name,
      framework: config.framework,
      mode: "fullstack",
      message: "MegaGate fullstack API scaffold ready",
    });
  });

  return app;
}
