import { AppShell } from "../components/AppShell";
import { getEngineBridge } from "../bridges/engine";

export default function HomePage() {
  getEngineBridge().run("render-shell");
  return <AppShell />;
}
