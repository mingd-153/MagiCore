import { runtimeProfile } from "../config/runtime";

export type EngineBridgeResult = {
  task: string;
  mode: "compatibility-shell";
  message: string;
};

export function getEngineBridge() {
  return {
    status: runtimeProfile.engine.bridge,
    crate: runtimeProfile.engine.crate,
    run(task: string): EngineBridgeResult {
      return {
        task,
        mode: "compatibility-shell",
        message:
          "Rust engine is scaffolded and ready to be wired when the workload needs it.",
      };
    },
  };
}
