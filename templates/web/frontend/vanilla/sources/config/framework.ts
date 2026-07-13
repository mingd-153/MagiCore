export type FrameworkConfig = {
  shortName: string;
  docs: {
    label: string;
    href: string;
  };
  signal: string[];
};

export const frameworkConfig: FrameworkConfig = {
  shortName: "Vanilla",
  docs: {
    label: "Explore Web APIs",
    href: "https://developer.mozilla.org/en-US/docs/Web/API",
  },
  signal: ["Framework-free", "Rust-ready", "Powered by mg"],
};
