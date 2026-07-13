import { brandConfig, type BrandLink } from "../config/brand";
import { frameworkConfig } from "../config/framework";

export type ProjectLinks = {
  primaryLink: BrandLink;
  secondaryLink: BrandLink;
};

export function useProjectLinks(): ProjectLinks {
  return {
    primaryLink: brandConfig.links.github,
    secondaryLink: frameworkConfig.docs,
  };
}
