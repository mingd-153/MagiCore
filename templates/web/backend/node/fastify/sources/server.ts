import Fastify from "fastify";
import { buildApp } from "./lib/app";

const app = buildApp(Fastify({ logger: true }));

app.listen({ port: Number(process.env.PORT ?? 3000), host: "0.0.0.0" }).catch((error) => {
  app.log.error(error);
  process.exit(1);
});
