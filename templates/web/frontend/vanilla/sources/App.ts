import { renderRoute } from "./router/AppRouter";

export function renderApp(root: Element | null): void {
  if (!root) {
    throw new Error("Missing #app root element");
  }

  root.innerHTML = renderRoute();
}
