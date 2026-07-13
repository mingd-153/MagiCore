import { brandConfig } from "../config/brand";
import { frameworkConfig } from "../config/framework";

export const siteContent = {
  eyebrow: `${brandConfig.productName} monorepo frontend`,
  welcome: "Welcome",
  title: `${brandConfig.productName} + ${frameworkConfig.shortName}`,
  subtitle:
    "A workspace-ready frontend shell with shared structure, clean package lanes, and less drag around the actual product work.",
  slogan: "One workspace. One clear starting point.",
  signal: frameworkConfig.signal,
  footer:
    "A frontend shell that fits a larger workspace without collapsing into clutter.",
};
