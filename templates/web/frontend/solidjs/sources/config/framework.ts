export type FrameworkConfig = {
  shortName: string;
  docs: {
    label: string;
    href: string;
  };
  signal: string[];
};

export const frameworkConfig: FrameworkConfig = {
  shortName: "Solid",
  docs: {
    label: "Explore Solid",
    href: "https://docs.solidjs.com",
  },
  signal: ["Solid-first", "Rust-ready", "Powered by mg"],
};
