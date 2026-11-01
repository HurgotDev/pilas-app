import { defineConfig } from "vite";
import { resolve } from "node:path";

// Tauri serves dist/ in production and localhost:1420 in development.
export default defineConfig({
  root: "ui",
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "safari16",
    rollupOptions: {
      input: {
        main: resolve(__dirname, "ui/index.html"),
        playground: resolve(__dirname, "ui/playground.html"),
        settings: resolve(__dirname, "ui/settings.html"),
      },
    },
  },
});
