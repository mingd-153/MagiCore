import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query";
import type { App } from "vue";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 1000 * 60 * 5 } },
});

export function installQueryPlugin(app: App) {
  app.use(VueQueryPlugin, { queryClient });
}