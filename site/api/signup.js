// POST /api/signup: stores the email DiskPilot asks for on first launch,
// with the app version, macOS version and CPU architecture. Nothing else is
// accepted. The table is created on first use. See /privacy.
import { neon } from "@neondatabase/serverless";

const EMAIL = /^[^\s@]{1,64}@[^\s@.][^\s@]*\.[^\s@.]+$/;
const VERSION = /^[0-9A-Za-z.+-]{1,32}$/;
const ARCH = new Set(["aarch64", "x86_64", "arm64", "x64"]);

function clean(body) {
  if (!body || typeof body !== "object") return null;
  const email = typeof body.email === "string" ? body.email.trim().toLowerCase() : "";
  if (email.length > 254 || !EMAIL.test(email)) return null;
  const version = (v) => (typeof v === "string" && VERSION.test(v) ? v : null);
  return {
    email,
    appVersion: version(body.appVersion),
    osVersion: version(body.osVersion),
    arch: ARCH.has(body.arch) ? body.arch : null,
  };
}

export default async function handler(req, res) {
  res.setHeader("Cache-Control", "no-store");
  if (req.method !== "POST") {
    res.setHeader("Allow", "POST");
    return res.status(405).json({ error: "Use POST." });
  }
  // Only the app signs up; it identifies itself as DiskPilot/<version>.
  if (!/^DiskPilot\//.test(req.headers["user-agent"] ?? "")) {
    return res.status(403).json({ error: "Sign up from the DiskPilot app." });
  }
  const url = process.env.DATABASE_URL ?? process.env.POSTGRES_URL;
  if (!url) return res.status(503).json({ error: "Signups are not set up yet." });

  let body = req.body;
  if (typeof body === "string") {
    try {
      body = JSON.parse(body);
    } catch {
      body = null;
    }
  }
  const s = clean(body);
  if (!s) return res.status(400).json({ error: "Enter a valid email address." });

  try {
    const sql = neon(url);
    await sql`CREATE TABLE IF NOT EXISTS signups (
      email text PRIMARY KEY,
      app_version text,
      os_version text,
      arch text,
      first_seen timestamptz NOT NULL DEFAULT now(),
      last_seen timestamptz NOT NULL DEFAULT now(),
      times integer NOT NULL DEFAULT 1
    )`;
    await sql`INSERT INTO signups (email, app_version, os_version, arch)
      VALUES (${s.email}, ${s.appVersion}, ${s.osVersion}, ${s.arch})
      ON CONFLICT (email) DO UPDATE SET
        app_version = EXCLUDED.app_version,
        os_version = EXCLUDED.os_version,
        arch = EXCLUDED.arch,
        last_seen = now(),
        times = signups.times + 1`;
    return res.status(204).end();
  } catch (e) {
    console.error("signup failed", e);
    return res.status(500).json({ error: "Could not save the signup. Try again later." });
  }
}
