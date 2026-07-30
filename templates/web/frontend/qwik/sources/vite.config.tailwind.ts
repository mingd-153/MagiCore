import { defineConfig } from "vite";
import { qwikCity } from "@builder.io/qwik-city/vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [tailwindcss(), qwikCity()],
});