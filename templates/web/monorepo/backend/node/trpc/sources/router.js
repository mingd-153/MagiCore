import { router, publicProcedure } from "./trpc.js";
import { z } from "zod";

export const appRouter = router({
  hello: publicProcedure
    .input(z.object({ name: z.string().optional() }))
    .query(({ input }) => ({
      greeting: `Hello ${input.name ?? "world"}`,
    })),
  health: publicProcedure.query(async () => ({ status: "ok" })),
});

export type AppRouter = typeof appRouter;