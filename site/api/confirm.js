// GET /api/confirm?token=…  shows a page with a Confirm button.
// POST /api/confirm (form: token=…) confirms the address.
// The two steps keep mail scanners, which open every link, from confirming
// addresses nobody clicked.
import { neon } from "@neondatabase/serverless";

import { confirmToken, migrate, validToken } from "../lib/confirm.js";

const page = (title, body) => `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex"><title>${title} · DiskPilot</title>
<style>
  body{margin:0;min-height:100vh;display:grid;place-items:center;background:#f6f6f9;color:#111114;font:16px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
  main{max-width:440px;margin:24px;padding:36px;border-radius:18px;background:#fff;box-shadow:0 0 0 1px rgba(17,17,20,.08),0 20px 50px rgba(40,36,90,.10)}
  h1{font-size:24px;margin:0 0 10px}p{margin:0 0 18px;color:#5d5e6c}
  button{font:inherit;font-weight:600;padding:10px 20px;border:0;border-radius:10px;background:#5b55e0;color:#fff;cursor:pointer}
  a{color:#5b55e0}
</style></head><body><main>${body}</main></body></html>`;

function send(res, status, html) {
  res.setHeader("Content-Type", "text/html; charset=utf-8");
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("Referrer-Policy", "no-referrer");
  return res.status(status).send(html);
}

const done = (ok) =>
  ok
    ? page(
        "Email confirmed",
        `<h1>You're confirmed</h1><p>Thanks. You'll hear about fixes and new versions of DiskPilot, nothing else.</p><p><a href="/">diskpilot.live</a></p>`,
      )
    : page(
        "Link expired",
        `<h1>This link doesn't work any more</h1><p>It may have been used already or be more than two days old. DiskPilot sends a new one the next time it opens.</p>`,
      );

function formToken(body) {
  if (body && typeof body === "object") return body.token;
  if (typeof body === "string") return new URLSearchParams(body).get("token");
  return null;
}

export default async function handler(req, res) {
  if (req.method === "GET") {
    const token = req.query?.token;
    if (!validToken(token)) return send(res, 400, done(false));
    return send(
      res,
      200,
      page(
        "Confirm your email",
        `<h1>Confirm your email</h1><p>Confirm that you want to hear about fixes and new versions of DiskPilot.</p>
<form method="post" action="/api/confirm"><input type="hidden" name="token" value="${token}"><button type="submit">Confirm my email</button></form>`,
      ),
    );
  }
  if (req.method !== "POST") {
    res.setHeader("Allow", "GET, POST");
    return res.status(405).end();
  }
  const url = process.env.DATABASE_URL ?? process.env.POSTGRES_URL;
  if (!url) return send(res, 503, done(false));
  try {
    const sql = neon(url);
    await migrate(sql);
    return send(res, 200, done(await confirmToken(sql, formToken(req.body))));
  } catch (e) {
    console.error("confirm failed", e);
    return send(res, 500, done(false));
  }
}
