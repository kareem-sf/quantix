import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("./", import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.join(root, "src"),
    },
  },
  clearScreen: false,
  build: {
    rollupOptions: {
      input: {
        main: path.join(root, "index.html"),
        // Shown in the desktop app's transparent launch window.
        splash: path.join(root, "splash.html"),
      },
    },
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/backend/**",
        "**/.quantix-dev/**",
        "**/docs/**",
      ],
    },
  },
  envPrefix: ["VITE_"],
});
