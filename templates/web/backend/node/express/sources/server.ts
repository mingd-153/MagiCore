import type { Express } from "express";
import express from "express";
import { buildApp } from "./lib/app.js";

const port = Number(process.env.PORT ?? 3000);
const app: Express = buildApp(express());

app.listen(port, "0.0.0.0", () => {
  console.log(`Server running on http://0.0.0.0:${port}`);
});
