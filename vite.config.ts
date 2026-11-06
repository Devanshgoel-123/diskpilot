/// <reference types="vitest/config" />
import path from "node:path";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import fs from "node:fs";
import { defineConfig, type Plugin } from "vite";

const host = process.env.TAURI_DEV_HOST;

// The local preview harness reads __preview*.json from public/. Those files
// hold real scan results from the developer's own disk, and everything in
// dist/ is embedded in the app, so they must never reach a build.
function dropPreviewData(): Plugin {
  let outDir = "dist";
  return {
    name: "drop-preview-data",
    apply: "build",
    configResolved(config) {
      outDir = path.resolve(config.root, config.build.outDir);
    },
    closeBundle() {
      for (const name of fs.readdirSync(outDir)) {
        if (name.startsWith("__preview")) fs.rmSync(path.join(outDir, name));
      }
    },
  };
}

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react(), tailwindcss(), dropPreviewData()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "safari15",
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // Assets load from disk inside the app bundle; chunk size is not a network concern.
    chunkSizeWarningLimit: 1500,
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
