export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Solid",
  docs: { label: "Explore Solid", href: "https://solidjs.com" },
  signal: ["Solid-first", "Fine-grained", "Powered by mg"],
};
