import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 1000 * 60 * 5 } },
});

export function installQueryPlugin(app) {
  app.use(VueQueryPlugin, { queryClient });
}