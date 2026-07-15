import Fastify from "fastify";

export const app = Fastify({ logger: false });

app.get("/health", async () => ({ status: "ok" }));
