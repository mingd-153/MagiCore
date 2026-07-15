import { router, publicProcedure } from "./trpc.js";
export const appRouter = router({ greeting: publicProcedure.query(() => "hello") });
