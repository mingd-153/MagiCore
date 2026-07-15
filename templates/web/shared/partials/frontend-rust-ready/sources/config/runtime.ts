export type RuntimeProfile = {
  architecture: "rust-first";
  compatibilityLayer: "js" | "ts";
  engine: {
    mode: "rust-ready";
    bridge: "dormant";
    crate: string;
  };
};

export const runtimeProfile: RuntimeProfile = {
  architecture: "rust-first",
  compatibilityLayer: "ts",
  engine: {
    mode: "rust-ready",
    bridge: "dormant",
    crate: "crates/engine",
  },
};
