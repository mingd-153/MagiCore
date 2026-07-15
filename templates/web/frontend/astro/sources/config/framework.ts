export type FrameworkConfig = {
  shortName: string;
  docs: { label: string; href: string };
  signal: string[];
};
export const frameworkConfig: FrameworkConfig = {
  shortName: "Astro",
  docs: { label: "Explore Astro", href: "https://astro.build" },
  signal: ["Content-first", "Islands", "Powered by mg"],
};
