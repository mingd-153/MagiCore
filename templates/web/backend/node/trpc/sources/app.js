import express from "express";
import { createExpressMiddleware } from "@trpc/server/adapters/express";
import { appRouter } from "../router.js";
import { createContext } from "../context.js";
export const app = express();
app.use("/trpc", createExpressMiddleware({ router: appRouter, createContext }));
app.get("/health", (_req, res) => res.json({ status: "ok" }));
