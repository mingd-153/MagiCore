import { getEngineBridge } from "../bridges/engine";
import { renderAppShell } from "../components/AppShell";

export function renderRoute() {
  getEngineBridge().run("render-shell");
  return renderAppShell();
}
