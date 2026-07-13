import { AppShell } from "../components/AppShell";
import { getEngineBridge } from "../bridges/engine";

export function AppRouter() {
  getEngineBridge().run("render-shell");
  return <AppShell />;
}
