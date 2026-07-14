export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Qwik",
  docs: { label: "Explore Qwik", href: "https://qwik.dev" },
  signal: ["Resumable", "Instant", "Powered by mg"],
};
