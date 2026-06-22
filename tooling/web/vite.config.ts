import { defineConfig } from "vite";

// Relative base so the built app can be served from any sub-path (e.g. GitHub Pages).
export default defineConfig({
  base: "./",
  build: {
    target: "esnext",
  },
});
