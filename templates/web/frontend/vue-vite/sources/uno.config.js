import { defineConfig } from "unocss";

export default defineConfig({
  presets: [require("@unocss/preset-uno")(), require("@unocss/preset-attributify")()],
});