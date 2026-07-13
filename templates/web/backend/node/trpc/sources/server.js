import { createServer } from "node:http";
import { buildApp } from "./lib/app.js";

const port = Number(process.env.PORT ?? 3000);
const app = buildApp();

createServer(app).listen(port, "0.0.0.0", () => {
  console.log(`tRPC server running on http://0.0.0.0:${port}`);
});