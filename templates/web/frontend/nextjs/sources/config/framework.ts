export type FrameworkConfig = {
  shortName: string;
  docs: {
    label: string;
    href: string;
  };
  signal: string[];
};

export const frameworkConfig: FrameworkConfig = {
  shortName: "Next.js",
  docs: {
    label: "Explore Next.js",
    href: "https://nextjs.org/docs",
  },
  signal: ["App Router", "Rust-ready", "Powered by mg"],
};
