# DiskPilot website

The landing page for DiskPilot. A static Vite site with self-hosted fonts and
Vercel Web Analytics.

## Develop

```bash
cd site
pnpm install
pnpm dev
```

## Pages

`index.html` is the landing page. Everything else is generated before each
build by `build/pages.mjs`, reusing the landing page's header, footer and icons:

| Page                                         | Source                                                       |
| -------------------------------------------- | ------------------------------------------------------------ |
| `/guides/<name>`                             | `content/guides/<name>.md`                                   |
| `/compare/<name>`                            | `content/compare/<name>.md`                                  |
| `/changelog`                                 | `../docs/releases/v*.md`, released versions only             |
| `/shortcuts`                                 | `../src/lib/shortcuts.json`, shortcuts already released only |
| `/guides`, `/compare`, `/404`, `sitemap.xml` | made from the above                                          |

Each Markdown page starts with front matter: `title` (the browser title),
`h1`, `description`, and optionally `summary` (for its card), `nav` (its
footer link), `order` and `updated` (YYYY-MM-DD). Generated files are ignored
by git; never edit them.

## Download count and stars

The hero shows the total `DiskPilot.dmg` downloads (rounded down, so 299 reads
"290+") and GitHub stars. They are fetched from the GitHub API while the site
builds. If GitHub cannot be reached, `stats-snapshot.json` is used instead, and
builds with `DISKPILOT_OFFLINE=1` (as in CI) always use it. Refresh it with
`node build/pages.mjs --snapshot`. Set `GITHUB_TOKEN` in Vercel to avoid the
API's rate limit for anonymous requests.

The `Refresh website` workflow rebuilds the site daily and after each
release, once a Vercel deploy hook is saved as the `VERCEL_DEPLOY_HOOK`
repository secret.

## Deploy on Vercel

1. Import the repository in Vercel and set **Root Directory** to `site`.
   `vercel.json` sets the framework, build command and output directory.
   Keep **Include files outside the root directory in the Build Step** on: the
   changelog and shortcuts pages read `../docs` and `../src`.
2. In the project, open **Analytics** and select **Enable**. The analytics
   routes become available after the next deployment.
3. Deploy. Page views appear in the Analytics tab. Clicks on GitHub links are
   sent as the custom event `github` with a `where` property; custom events
   are shown on plans that include them.

Analytics only runs on the deployed site. Locally, the package logs events to
the browser console instead.
