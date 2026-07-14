import { initTRPC } from "@trpc/server";
import { z } from "zod";

const t = initTRPC.create();

export const router = t.router({
  hello: t.procedure
    .input(z.object({ name: z.string().optional() }))
    .query(({ input }) => ({ greeting: `Hello ${input.name ?? "world"}!` })),
});

export type AppRouter = typeof router;