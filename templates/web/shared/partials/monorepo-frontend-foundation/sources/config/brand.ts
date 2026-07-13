export type BrandLink = {
  label: string;
  href: string;
};

export type BrandConfig = {
  productName: string;
  logoPath: string;
  links: {
    github: BrandLink;
  };
};

export const brandConfig: BrandConfig = {
  productName: "MegaGate",
  logoPath: "/megagate-logo.svg",
  links: {
    github: {
      label: "View MegaGate on GitHub",
      href: "https://github.com/mingd-153/MegaGate",
    },
  },
};
