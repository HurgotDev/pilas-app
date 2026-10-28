import { defineConfig } from "vite";
import { resolve } from "node:path";

// La UI vive en ui/; Tauri sirve dist/ en producción y localhost:1420 en desarrollo.
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
