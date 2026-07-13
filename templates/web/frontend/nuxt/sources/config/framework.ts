export type FrameworkConfig = {
  shortName: string;
  docs: {
    label: string;
    href: string;
  };
  signal: string[];
};

export const frameworkConfig: FrameworkConfig = {
  shortName: "Nuxt",
  docs: {
    label: "Explore Nuxt",
    href: "https://nuxt.com",
  },
  signal: ["Vue-first", "SSR-ready", "Powered by mg"],
};
