import { z } from "zod";
import { router, publicProcedure } from "./trpc.js";

export const appRouter = router({
  greeting: publicProcedure
    .input(z.object({ name: z.string().optional() }).optional())
    .query(({ input }) => `hello ${input?.name ?? "megagate"}`),
});

export type AppRouter = typeof appRouter;
