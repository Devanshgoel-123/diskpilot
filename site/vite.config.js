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
        // Preload the two Latin font files every page uses, so text renders in
        // Geist on first paint instead of waiting for the stylesheet to ask.
        name: "diskpilot-font-preload",
        transformIndexHtml: {
          order: "post",
          handler(_html, ctx) {
            if (!ctx.bundle) return [];
            return Object.keys(ctx.bundle)
              .filter((f) => /geist(-mono)?-latin-wght-normal-[^/]+\.woff2$/.test(f))
              .map((f) => ({
                tag: "link",
                attrs: {
                  rel: "preload",
                  href: `/${f}`,
                  as: "font",
                  type: "font/woff2",
                  crossorigin: "",
                },
                injectTo: "head",
              }));
          },
        },
      },
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
