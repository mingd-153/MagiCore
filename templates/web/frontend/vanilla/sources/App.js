import { AppRouter } from "./router/AppRouter";
export function App(root) {
  const h1 = document.createElement("h1");
  h1.textContent = "{{project_name}}";
  root.appendChild(h1);
  AppRouter(root);
}
