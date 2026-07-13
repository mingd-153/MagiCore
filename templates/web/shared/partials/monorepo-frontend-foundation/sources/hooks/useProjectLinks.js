import { brandConfig } from "../config/brand";
import { frameworkConfig } from "../config/framework";

export function useProjectLinks() {
  return {
    primaryLink: brandConfig.links.github,
    secondaryLink: frameworkConfig.docs,
  };
}
