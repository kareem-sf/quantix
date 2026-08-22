import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// Tauri exposes the target platform to its Vite hooks. Falling back to the
// host platform keeps the browser preview faithful during local Windows work.
// @ts-expect-error process is a nodejs global
const targetPlatform = process.env.TAURI_ENV_PLATFORM ?? process.platform;
const windowsTitleBar = ["windows", "win32"].includes(targetPlatform);

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  define: {
    __QUANTIX_WINDOWS_TITLEBAR__: JSON.stringify(windowsTitleBar),
  },

  // Keep the startup splash as a separate, dependency-free entry point. The
  // main React bundle is still the only entry that owns the workspace.
  build: {
    rollupOptions: {
      input: {
        main: resolve(
          fileURLToPath(new URL(".", import.meta.url)),
          "index.html",
        ),
        splash: resolve(
          fileURLToPath(new URL(".", import.meta.url)),
          "splash.html",
        ),
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/.dev/**", "**/src-tauri/**"],
    },
  },
}));
