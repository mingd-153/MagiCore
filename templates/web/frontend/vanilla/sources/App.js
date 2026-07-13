import { renderRoute } from "./router/AppRouter";

export function renderApp(root) {
  if (!root) {
    throw new Error("Missing #app root element");
  }

  root.innerHTML = renderRoute();
}
