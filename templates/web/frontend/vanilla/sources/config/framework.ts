export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Vanilla",
  docs: { label: "Explore Web APIs", href: "https://developer.mozilla.org" },
  signal: ["Lightweight", "Zero-deps", "Powered by mg"],
};
