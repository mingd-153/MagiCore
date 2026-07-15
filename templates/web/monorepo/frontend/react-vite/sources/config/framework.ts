export type FrameworkConfig = {
  shortName: string;
  docs: {
    label: string;
    href: string;
  };
  signal: string[];
};

export const frameworkConfig: FrameworkConfig = {
  shortName: "React",
  docs: {
    label: "Explore React",
    href: "https://react.dev",
  },
  signal: ["Workspace-ready", "Rust-ready", "Powered by mg"],
};
