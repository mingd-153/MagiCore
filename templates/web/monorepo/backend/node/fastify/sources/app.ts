import { loadConfig } from "../config/app";
import { healthRoute } from "../routes/health";

type FastifyInstance = {
  get: (path: string, handler: () => Promise<unknown> | unknown) => void;
};

export function buildApp(app: FastifyInstance) {
  const config = loadConfig();

  app.get("/health", async () => healthRoute());
  app.get("/", async () => ({
    service: config.name,
    framework: config.framework,
    message: "MegaGate backend scaffold ready"
  }));

  return app;
}
