import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      input: {
        settings: resolve(__dirname, "index.html"),
        overlay: resolve(__dirname, "overlay.html"),
        osd: resolve(__dirname, "osd.html"),
      },
    },
    outDir: "dist",
  },
  server: {
    port: 1420,
    strictPort: true,
  },
});
