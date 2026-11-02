# DiskPilot website

The landing page for DiskPilot. A static Vite site with self-hosted fonts and
Vercel Web Analytics.

## Develop

```bash
cd site
pnpm install
pnpm dev
```

## Deploy on Vercel

1. Import the repository in Vercel and set **Root Directory** to `site`.
   `vercel.json` sets the framework, build command and output directory.
2. In the project, open **Analytics** and select **Enable**. The analytics
   routes become available after the next deployment.
3. Deploy. Page views appear in the Analytics tab. Clicks on GitHub links are
   sent as the custom event `github` with a `where` property; custom events
   are shown on plans that include them.

Analytics only runs on the deployed site. Locally, the package logs events to
the browser console instead.
