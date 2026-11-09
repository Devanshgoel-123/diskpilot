import { existsSync } from "node:fs";
import { join } from "node:path";

import { defineConfig } from "vite";

import { generate } from "./build/pages.mjs";

// The guides, comparisons, changelog and shortcuts pages are generated before
// each build (see build/pages.mjs), then built by Vite like index.html.
export default defineConfig(async () => {
  const { inputs, fill } = await generate();
  return {
    build: { rollupOptions: { input: inputs } },
    plugins: [
      {
        name: "diskpilot-pages",
        transformIndexHtml: (html) => fill(html),
        // Vercel serves /changelog from changelog.html (cleanUrls); do the same in development.
        configureServer(server) {
          server.middlewares.use((req, _res, next) => {
            const path = (req.url ?? "").split("?")[0];
            if (
              path !== "/" &&
              !path.includes(".") &&
              existsSync(join(server.config.root, `${path}.html`))
            ) {
              req.url = `${path}.html`;
            }
            next();
          });
        },
      },
    ],
  };
});
