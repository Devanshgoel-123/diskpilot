// Builds the site's extra pages from Markdown and repository data, so they
// cannot drift from the app:
//
//   content/guides/*.md, content/compare/*.md  ->  /guides/<name>, /compare/<name>
//   ../docs/releases/v*.md                      ->  /changelog
//   ../src/lib/shortcuts.json                   ->  /shortcuts (released shortcuts only)
//   GitHub releases and repository              ->  download count and stars
//
// Every page reuses the header, footer and icons of index.html. The pages are
// written next to index.html (and ignored by git) so Vite builds them like any
// other page. `node build/pages.mjs --snapshot` refreshes stats-snapshot.json,
// which is used whenever GitHub cannot be reached during a build.

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Marked } from "marked";

export const SITE = "https://diskpilot.live";
export const REPO = "Devanshgoel-123/diskpilot";
const GITHUB = `https://github.com/${REPO}`;
const DMG = `${GITHUB}/releases/latest/download/DiskPilot.dmg`;

const siteDir = dirname(dirname(fileURLToPath(import.meta.url)));
const repoDir = dirname(siteDir);
const SNAPSHOT = join(siteDir, "stats-snapshot.json");

/** Generated files, relative to the site folder. Also listed in .gitignore. */
const GENERATED = [
  "changelog.html",
  "shortcuts.html",
  "guides.html",
  "compare.html",
  "404.html",
  "guides",
  "compare",
  "public/sitemap.xml",
];

// ---------- Helpers ----------

export function esc(s) {
  return String(s)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function slug(s) {
  return String(s)
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/&[a-z]+;/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

/** Compares "0.6.10" and "v0.6.2" as numbers. */
export function compareVersions(a, b) {
  const p = (v) =>
    String(v)
      .replace(/^v/, "")
      .split(/[.+-]/)
      .slice(0, 3)
      .map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [p(a), p(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
}

/** "290+" for 299, so the number never overstates and does not jitter. */
export function roundDown(n) {
  if (n >= 1000) return `${(Math.floor(n / 100) / 10).toLocaleString("en-US")}k+`;
  if (n >= 100) return `${Math.floor(n / 10) * 10}+`;
  return String(n);
}

function formatDate(iso) {
  if (!iso) return "";
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  });
}

/** Simple `key: value` front matter between --- lines. */
export function frontMatter(text) {
  const m = /^---\n([\s\S]*?)\n---\n?/.exec(text);
  if (!m) return { meta: {}, body: text };
  const meta = {};
  for (const line of m[1].split("\n")) {
    const i = line.indexOf(":");
    if (i > 0) meta[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return { meta, body: text.slice(m[0].length) };
}

/** Markdown to HTML with heading ids, scrollable tables and safe external links. */
export function renderMarkdown(md) {
  const headings = [];
  const marked = new Marked({ gfm: true });
  marked.use({
    renderer: {
      heading({ tokens, depth }) {
        const text = this.parser.parseInline(tokens);
        const id = slug(text);
        if (depth === 2) headings.push({ id, text });
        return `<h${depth} id="${id}">${text}</h${depth}>\n`;
      },
      table(token) {
        return `<div class="doc-table">${marked.Renderer.prototype.table.call(this, token)}</div>\n`;
      },
      link({ href, title, tokens }) {
        const text = this.parser.parseInline(tokens);
        const external = /^https?:\/\//.test(href) && !href.startsWith(SITE);
        const t = title ? ` title="${esc(title)}"` : "";
        const ext = external ? ' target="_blank" rel="noopener"' : "";
        return `<a href="${esc(href)}"${t}${ext}>${text}</a>`;
      },
    },
  });
  return { html: marked.parse(md), headings };
}

/**
 * Drops sections about downloading, verifying or installing, which only make
 * sense on GitHub (the same rule as the app's "What's new").
 */
export function withoutInstallSections(md) {
  const out = [];
  let skip = null;
  let code = false;
  for (const line of md.split("\n")) {
    if (line.startsWith("```")) code = !code;
    const h = code ? null : /^(#{2,6}) (.*)$/.exec(line);
    if (h) {
      const level = h[1].length;
      if (skip !== null && level <= skip) skip = null;
      if (skip === null && /^(Download|Verify|Install)/.test(h[2])) skip = level;
    }
    if (skip === null) out.push(line);
  }
  return out.join("\n").trim();
}

// ---------- Stats from GitHub ----------

async function github(path) {
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "diskpilot-site-build" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const res = await fetch(`https://api.github.com/${path}`, {
    headers,
    signal: AbortSignal.timeout(8000),
  });
  if (!res.ok) throw new Error(`GitHub ${path}: ${res.status}`);
  return res.json();
}

/** Download count, stars and release dates, live or from the snapshot. */
export async function loadStats() {
  const snapshot = JSON.parse(readFileSync(SNAPSHOT, "utf8"));
  if (process.env.DISKPILOT_OFFLINE) return { ...snapshot, live: false };
  try {
    const [releases, repo] = await Promise.all([
      github(`repos/${REPO}/releases?per_page=100`),
      github(`repos/${REPO}`),
    ]);
    const published = releases.filter((r) => !r.draft && !r.prerelease);
    const stats = {
      downloads: published.reduce(
        (sum, r) =>
          sum +
          r.assets
            .filter((a) => a.name === "DiskPilot.dmg")
            .reduce((s, a) => s + a.download_count, 0),
        0,
      ),
      stars: repo.stargazers_count,
      releases: published
        .map((r) => ({ tag: r.tag_name, published: r.published_at }))
        .sort((a, b) => compareVersions(b.tag, a.tag)),
      checked: new Date().toISOString(),
    };
    return { ...stats, live: true };
  } catch (e) {
    console.warn(`[pages] Using stats-snapshot.json: ${e.message}`);
    return { ...snapshot, live: false };
  }
}

// ---------- Shared page parts, taken from index.html ----------

function chrome() {
  const index = readFileSync(join(siteDir, "index.html"), "utf8");
  const pick = (re, what) => {
    const m = re.exec(index);
    if (!m) throw new Error(`[pages] index.html has no ${what}`);
    return m[0];
  };
  const sprite = pick(/<svg class="sprite"[\s\S]*?<\/svg>\n/, "icon sprite");
  // On other pages, section links point back to the homepage.
  const header = pick(/<header class="nav"[\s\S]*?<\/header>/, "header")
    .replace(
      'href="#top" aria-label="DiskPilot, back to top"',
      'href="/" aria-label="DiskPilot home"',
    )
    // Only links: icons in the sprite are also referenced as href="#i-…".
    .replace(/(<a\b[^>]*?)href="#([a-z-]+)"/g, '$1href="/#$2"');
  const footer = pick(/<footer class="footer"[\s\S]*?<\/footer>/, "footer");
  return { sprite, header, footer };
}

// ---------- Content ----------

function readCollection(dir, base) {
  const folder = join(siteDir, "content", dir);
  if (!existsSync(folder)) return [];
  return readdirSync(folder)
    .filter((f) => f.endsWith(".md"))
    .map((f) => {
      const { meta, body } = frontMatter(readFileSync(join(folder, f), "utf8"));
      const name = f.replace(/\.md$/, "");
      for (const key of ["title", "description", "h1"]) {
        if (!meta[key])
          throw new Error(`[pages] content/${dir}/${f} needs "${key}" in its front matter`);
      }
      return { ...meta, name, path: `/${base}/${name}`, body, order: Number(meta.order ?? 99) };
    })
    .sort((a, b) => a.order - b.order || a.h1.localeCompare(b.h1));
}

function releaseNotes(stats) {
  const folder = join(repoDir, "docs", "releases");
  if (!existsSync(folder)) {
    throw new Error(
      "[pages] ../docs/releases is missing. On Vercel, keep 'Include files outside the root directory' on.",
    );
  }
  const dates = new Map(stats.releases.map((r) => [r.tag, r.published]));
  return (
    readdirSync(folder)
      .filter((f) => /^v\d+\.\d+\.\d+\.md$/.test(f))
      .map((f) => {
        const tag = f.replace(/\.md$/, "");
        const md = withoutInstallSections(readFileSync(join(folder, f), "utf8"))
          .split("\n")
          .filter((l) => !/^## What's changed\s*$/.test(l))
          .map((l) => l.replace(/^## /, "### "))
          .join("\n");
        return { tag, published: dates.get(tag) ?? null, md };
      })
      // Only versions that were released (a tag being prepared is left out).
      .filter((r) => r.published || !stats.releases.length)
      .sort((a, b) => compareVersions(b.tag, a.tag))
  );
}

function shortcuts(latest) {
  const data = JSON.parse(readFileSync(join(repoDir, "src", "lib", "shortcuts.json"), "utf8"));
  return data.groups
    .map((g) => ({
      ...g,
      items: g.items.filter((s) => !latest || compareVersions(s.since, latest) <= 0),
    }))
    .filter((g) => g.items.length);
}

// ---------- Page template ----------

function page({ path, title, description, body, crumbs = [], ld = [] }, parts) {
  const url = `${SITE}${path === "/" ? "/" : path}`;
  const breadcrumbs = crumbs.length
    ? {
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        itemListElement: [{ name: "DiskPilot", path: "/" }, ...crumbs].map((c, i) => ({
          "@type": "ListItem",
          position: i + 1,
          name: c.name,
          item: `${SITE}${c.path}`,
        })),
      }
    : null;
  const json = [breadcrumbs, ...ld]
    .filter(Boolean)
    .map(
      (d) =>
        `<script type="application/ld+json">${JSON.stringify(d).replaceAll("<", "\\u003c")}</script>`,
    )
    .join("\n    ");
  const trail = crumbs.length
    ? `<nav class="crumbs" aria-label="Breadcrumb"><a href="/">DiskPilot</a>${crumbs
        .map((c, i) =>
          i === crumbs.length - 1
            ? `<span aria-current="page">${esc(c.name)}</span>`
            : `<a href="${c.path}">${esc(c.name)}</a>`,
        )
        .join("")}</nav>`
    : "";
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <title>${esc(title)}</title>
    <meta name="description" content="${esc(description)}" />
    <meta name="theme-color" content="#121316" />
    <meta name="color-scheme" content="dark light" />
    <script src="/theme.js"></script>
    <link rel="canonical" href="${url}" />
    <meta property="og:type" content="article" />
    <meta property="og:url" content="${url}" />
    <meta property="og:site_name" content="DiskPilot" />
    <meta property="og:title" content="${esc(title)}" />
    <meta property="og:description" content="${esc(description)}" />
    <meta property="og:image" content="${SITE}/og-v2.png" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:creator" content="@0xgoel" />
    <link rel="icon" type="image/svg+xml" href="/logo.svg" />
    <link rel="apple-touch-icon" href="/apple-touch-icon.png" />
    ${json}
  </head>
  <body class="subpage">
    ${parts.sprite}
    ${parts.header}

    <main id="main" class="doc-main">
      <div class="wrap doc">
        ${trail}
        ${body}
      </div>
    </main>

    ${parts.footer}
    <script type="module" src="/src/page.js"></script>
  </body>
</html>
`;
}

function toc(headings) {
  if (headings.length < 3) return "";
  return `<nav class="toc" aria-label="On this page"><p>On this page</p><ol>${headings
    .map((h) => `<li><a href="#${h.id}">${h.text}</a></li>`)
    .join("")}</ol></nav>`;
}

const CTA = `<aside class="doc-cta">
          <div>
            <h2>Try it on your own Mac</h2>
            <p>Free and open source. DiskPilot only reads until you review and confirm a cleanup.</p>
          </div>
          <a class="btn btn-primary" href="${DMG}" data-track="download" data-where="doc">
            <svg class="ico" aria-hidden="true"><use href="#i-download" /></svg>Download for Mac
          </a>
        </aside>`;

const FIX = (path) =>
  `<p class="doc-fix">Something out of date or wrong? <a href="${GITHUB}/issues/new?title=${encodeURIComponent(
    `Website: ${path}`,
  )}" target="_blank" rel="noopener">Tell us on GitHub</a>.</p>`;

function articlePage(item, crumb, parts) {
  const { html, headings } = renderMarkdown(item.body);
  const updated = item.updated
    ? `<p class="doc-meta">Updated ${esc(formatDate(item.updated))}</p>`
    : "";
  const body = `<header class="doc-head">
          <h1>${esc(item.h1)}</h1>
          <p class="lede">${esc(item.description)}</p>
          ${updated}
        </header>
        ${toc(headings)}
        <div class="prose">${html}</div>
        ${CTA}
        ${FIX(item.path)}`;
  return page(
    {
      path: item.path,
      title: item.title,
      description: item.description,
      body,
      crumbs: [crumb, { name: item.h1, path: item.path }],
      ld: [
        {
          "@context": "https://schema.org",
          "@type": "TechArticle",
          headline: item.h1,
          description: item.description,
          dateModified: item.updated,
          author: { "@type": "Person", name: "Devansh Goel", url: "https://x.com/0xgoel" },
          publisher: { "@type": "Organization", name: "DiskPilot", url: SITE },
          mainEntityOfPage: `${SITE}${item.path}`,
        },
      ],
    },
    parts,
  );
}

function hubPage({ path, name, title, h1, description, items }, parts) {
  const cards = items
    .map(
      (i) =>
        `<li><a class="card-link" href="${i.path}"><h2>${esc(i.h1)}</h2><p>${esc(
          i.summary ?? i.description,
        )}</p></a></li>`,
    )
    .join("\n          ");
  const body = `<header class="doc-head">
          <h1>${esc(h1)}</h1>
          <p class="lede">${esc(description)}</p>
        </header>
        <ul class="card-list">
          ${cards}
        </ul>
        ${CTA}`;
  return page({ path, title, description, body, crumbs: [{ name, path }] }, parts);
}

function changelogPage(notes, parts) {
  const sections = notes
    .map((r) => {
      const { html } = renderMarkdown(r.md);
      const v = r.tag.replace(/^v/, "");
      const date = r.published
        ? `<time datetime="${r.published.slice(0, 10)}">${esc(formatDate(r.published))}</time>`
        : "";
      return `<section class="release-entry" id="${slug(r.tag)}" aria-labelledby="${slug(r.tag)}-title">
          <div class="release-side">
            <h2 id="${slug(r.tag)}-title"><a href="#${slug(r.tag)}">${esc(v)}</a></h2>
            ${date}
            <a class="release-link" href="${GITHUB}/releases/tag/${r.tag}" target="_blank" rel="noopener">On GitHub</a>
          </div>
          <div class="prose">${html}</div>
        </section>`;
    })
    .join("\n        ");
  const body = `<header class="doc-head">
          <h1>Changelog</h1>
          <p class="lede">What changed in each version of DiskPilot. DiskPilot updates itself from 0.6.0; the same notes appear in the app as What's new.</p>
        </header>
        ${sections}
        ${CTA}`;
  return page(
    {
      path: "/changelog",
      title: "DiskPilot changelog: what's new in each version",
      description:
        "Every DiskPilot release, newest first: new features, fixes and safety changes in the free, open-source Mac storage app.",
      body,
      crumbs: [{ name: "Changelog", path: "/changelog" }],
    },
    parts,
  );
}

function shortcutsPage(groups, parts) {
  const all = groups.flatMap((g) => g.items);
  const hasHelp = all.some((s) => s.command === "help");
  const kbd = (keys) => keys.map((k) => `<kbd>${esc(k)}</kbd>`).join("");
  const tables = groups
    .map(
      (g) => `<section aria-labelledby="${slug(g.title)}">
          <h2 id="${slug(g.title)}">${esc(g.title)}</h2>
          <div class="doc-table"><table class="keys">
            <thead><tr><th scope="col">Action</th><th scope="col">Keys</th></tr></thead>
            <tbody>
              ${g.items.map((s) => `<tr><td>${esc(s.action)}</td><td>${kbd(s.keys)}</td></tr>`).join("\n              ")}
            </tbody>
          </table></div>
        </section>`,
    )
    .join("\n        ");
  const body = `<header class="doc-head">
          <h1>Keyboard shortcuts</h1>
          <p class="lede">${
            hasHelp
              ? "Every shortcut in DiskPilot. In the app, press <kbd>⌘</kbd><kbd>/</kbd> to see this list."
              : "Every shortcut in the current version of DiskPilot."
          } ⌘ is the Command key.</p>
        </header>
        <div class="prose">
        ${tables}
        <p>Shortcuts that use ⌘ work anywhere in DiskPilot, except while a window such as a preview or a cleanup review is open: that window keeps the keyboard until you close it.</p>
        </div>
        ${CTA}`;
  return page(
    {
      path: "/shortcuts",
      title: "DiskPilot keyboard shortcuts for Mac",
      description:
        "Every DiskPilot keyboard shortcut: move between pages, scan again, filter lists and preview files without the mouse.",
      body,
      crumbs: [{ name: "Keyboard shortcuts", path: "/shortcuts" }],
    },
    parts,
  );
}

function notFoundPage(parts, guides) {
  const body = `<header class="doc-head">
          <h1>That page isn't here</h1>
          <p class="lede">It may have moved. These are the pages people look for most.</p>
        </header>
        <ul class="card-list">
          <li><a class="card-link" href="/"><h2>DiskPilot</h2><p>What it does, and the download.</p></a></li>
          <li><a class="card-link" href="/guides"><h2>Guides</h2><p>${esc(
            guides
              .map((g) => g.h1)
              .slice(0, 3)
              .join(" · "),
          )}</p></a></li>
          <li><a class="card-link" href="/changelog"><h2>Changelog</h2><p>What changed in each version.</p></a></li>
        </ul>`;
  return page(
    {
      path: "/404",
      title: "Page not found · DiskPilot",
      description: "This page does not exist.",
      body,
    },
    parts,
  );
}

// ---------- Index fill-ins ----------

function footerLinks(guides, compares) {
  const col = (title, links) =>
    `<div class="footer-col"><p>${esc(title)}</p><ul>${links
      .map(([text, href]) => {
        const ext = href.startsWith("http") ? ' target="_blank" rel="noopener"' : "";
        return `<li><a href="${esc(href)}"${ext}>${esc(text)}</a></li>`;
      })
      .join("")}</ul></div>`;
  return `<nav class="footer-links" aria-label="Site">
        ${col("DiskPilot", [
          ["Download", DMG],
          ["Install with Homebrew", "/#install"],
          ["Changelog", "/changelog"],
          ["Keyboard shortcuts", "/shortcuts"],
          ["Source on GitHub", GITHUB],
        ])}
        ${col("Guides", [...guides.map((g) => [g.nav ?? g.h1, g.path]), ["All guides", "/guides"]])}
        ${col("Compare", [...compares.map((c) => [c.nav ?? c.h1, c.path]), ["All comparisons", "/compare"]])}
        ${col("Help", [
          ["Report a problem", `${GITHUB}/issues/new`],
          ["Ask a question", `${GITHUB}/discussions`],
          ["Privacy", "/#privacy"],
          ["Updates on X", "https://x.com/0xgoel"],
        ])}
      </nav>`;
}

function proof(stats) {
  const parts = [];
  if (stats.downloads > 0)
    parts.push(`<span><strong>${esc(roundDown(stats.downloads))}</strong> downloads</span>`);
  if (stats.stars > 0) {
    parts.push(
      `<a href="${GITHUB}/stargazers" target="_blank" rel="noopener" data-track="github" data-where="proof"><svg class="ico" aria-hidden="true"><use href="#i-star" /></svg><strong>${esc(stats.stars)}</strong> stars on GitHub</a>`,
    );
  }
  return parts.length ? `<p class="proof">${parts.join("")}</p>` : "";
}

function softwareLd(stats) {
  const latest = stats.releases[0];
  return `<script type="application/ld+json">${JSON.stringify({
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "DiskPilot",
    description:
      "Free, open-source Mac app that explains what fills your disk and cleans it up safely, with a preview before anything moves.",
    url: SITE,
    image: `${SITE}/og-v2.png`,
    applicationCategory: "UtilitiesApplication",
    operatingSystem: "macOS 11 or later",
    softwareVersion: latest?.tag?.replace(/^v/, ""),
    datePublished: latest?.published?.slice(0, 10),
    downloadUrl: DMG,
    license: `${GITHUB}/blob/main/LICENSE`,
    isAccessibleForFree: true,
    offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
    author: { "@type": "Person", name: "Devansh Goel", url: "https://x.com/0xgoel" },
  }).replaceAll("<", "\\u003c")}</script>`;
}

// ---------- Entry point ----------

/**
 * Writes every generated page and returns the HTML inputs for Vite, plus a
 * function that fills the placeholders in any page (index.html included).
 */
export async function generate() {
  for (const g of GENERATED) rmSync(join(siteDir, g), { recursive: true, force: true });
  const stats = await loadStats();
  const latest = stats.releases[0]?.tag ?? null;
  const parts = chrome();
  const guides = readCollection("guides", "guides");
  const compares = readCollection("compare", "compare");

  const files = new Map();
  for (const g of guides)
    files.set(`guides/${g.name}.html`, articlePage(g, { name: "Guides", path: "/guides" }, parts));
  for (const c of compares)
    files.set(
      `compare/${c.name}.html`,
      articlePage(c, { name: "Compare", path: "/compare" }, parts),
    );
  files.set(
    "guides.html",
    hubPage(
      {
        path: "/guides",
        name: "Guides",
        title: "Mac storage guides for developers · DiskPilot",
        h1: "Guides",
        description:
          "Plain answers to the storage questions developers ask most: System Data, Docker, Xcode, node_modules, and getting started with DiskPilot.",
        items: guides,
      },
      parts,
    ),
  );
  files.set(
    "compare.html",
    hubPage(
      {
        path: "/compare",
        name: "Compare",
        title: "DiskPilot compared with other Mac storage tools",
        h1: "DiskPilot compared",
        description:
          "Honest comparisons with DaisyDisk, CleanMyMac, DevCleaner, OmniDiskSweeper, GrandPerspective, npkill and kondo, including where each one is the better choice.",
        items: compares,
      },
      parts,
    ),
  );
  files.set("changelog.html", changelogPage(releaseNotes(stats), parts));
  files.set("shortcuts.html", shortcutsPage(shortcuts(latest), parts));
  files.set("404.html", notFoundPage(parts, guides));

  for (const [rel, html] of files) {
    const full = join(siteDir, rel);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, html);
  }

  const urls = [
    { path: "/", lastmod: stats.releases[0]?.published },
    { path: "/changelog", lastmod: stats.releases[0]?.published },
    { path: "/shortcuts" },
    { path: "/guides" },
    ...guides.map((g) => ({ path: g.path, lastmod: g.updated })),
    { path: "/compare" },
    ...compares.map((c) => ({ path: c.path, lastmod: c.updated })),
  ];
  writeFileSync(
    join(siteDir, "public", "sitemap.xml"),
    `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls
  .map(
    (u) =>
      `  <url>\n    <loc>${SITE}${u.path}</loc>${u.lastmod ? `\n    <lastmod>${u.lastmod.slice(0, 10)}</lastmod>` : ""}\n  </url>`,
  )
  .join("\n")}
</urlset>
`,
  );

  const links = footerLinks(guides, compares);
  const fill = (html) =>
    html
      .replace("<!-- site:footer-links -->", links)
      .replace("<!-- site:proof -->", proof(stats))
      .replace("<!-- site:software-ld -->", softwareLd(stats));

  const inputs = { index: join(siteDir, "index.html") };
  for (const rel of files.keys()) inputs[rel.replace(/\.html$/, "")] = join(siteDir, rel);
  console.log(
    `[pages] ${files.size} pages, ${stats.downloads} downloads, ${stats.stars} stars (${stats.live ? "live" : "snapshot"})`,
  );
  return { inputs, fill, stats };
}

// `node build/pages.mjs --snapshot` saves today's numbers for offline builds.
if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv.includes("--snapshot")) {
  const stats = await loadStats();
  if (!stats.live) {
    console.error("[pages] GitHub could not be reached; the snapshot was not changed.");
    process.exit(1);
  }
  const { live: _live, ...keep } = stats;
  writeFileSync(SNAPSHOT, `${JSON.stringify(keep, null, 2)}\n`);
  console.log(`[pages] Saved ${SNAPSHOT}`);
}
