export type FrameworkConfig = {
  shortName: string;
  docs: {
    label: string;
    href: string;
  };
  signal: string[];
};

export const frameworkConfig: FrameworkConfig = {
  shortName: "Vue",
  docs: {
    label: "Explore Vue",
    href: "https://vuejs.org",
  },
  signal: ["Vue-first", "Rust-ready", "Powered by mg"],
};
