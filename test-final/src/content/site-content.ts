import { brandConfig } from "../config/brand";
import { frameworkConfig } from "../config/framework";

export const siteContent = {
  eyebrow: `${brandConfig.productName} web core`,
  welcome: "Welcome",
  title: `${brandConfig.productName} + ${frameworkConfig.shortName}`,
  subtitle:
    "A frontend starting point shaped for product work first: less setup noise, faster feedback, and a calmer surface to build on.",
  slogan: "Design the flow. Ship the product.",
  signal: frameworkConfig.signal,
  footer:
    "A starter that feels intentional on first run and stays readable when the app grows into something real.",
};
