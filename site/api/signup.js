// POST /api/signup: stores the email DiskPilot asks for on first launch,
// with the app version, macOS version and CPU architecture. Nothing else is
// accepted. The table is created on first use. See /privacy.
import { resolveMx } from "node:dns/promises";
import { neon } from "@neondatabase/serverless";

const EMAIL = /^[^\s@]{1,64}@[^\s@.][^\s@]*\.[^\s@.]+$/;
const VERSION = /^[0-9A-Za-z.+-]{1,32}$/;
const ARCH = new Set(["aarch64", "x86_64", "arm64", "x64"]);
// A signup is under 300 bytes; anything much bigger is not from the app.
const MAX_BODY = 2048;

// A few signups per address per 10 minutes is plenty for a real person.
// Counted in memory only, so no IP address is ever stored. This is per
// function instance; the Vercel Firewall rule on /api/signup is the hard cap.
const WINDOW_MS = 10 * 60 * 1000;
const PER_WINDOW = 5;
const hits = new Map();
export function limited(ip, now = Date.now()) {
  if (hits.size > 10_000) hits.clear();
  const h = hits.get(ip);
  if (!h || now - h.start > WINDOW_MS) {
    hits.set(ip, { start: now, n: 1 });
    return false;
  }
  h.n += 1;
  return h.n > PER_WINDOW;
}

// The same rules as the app (src-tauri/src/signup.rs), for anything that skips them.
const THROWAWAY = new Set([
  "example.com", "example.org", "example.net", "test.com", "xx.com", "xxx.com", "abc.com",
  "mailinator.com", "guerrillamail.com", "sharklasers.com", "10minutemail.com", "yopmail.com",
  "temp-mail.org", "tempmail.com", "trashmail.com", "maildrop.cc", "getnada.com",
  "dispostable.com", "dontsp.am", "throwawaymail.com", "fakeinbox.com", "mailnesia.com",
  "spamgourmet.com", "burnermail.io", "emailondeck.com", "mohmal.com",
]);
const RESERVED_TLDS = new Set(["test", "example", "invalid", "localhost", "local"]);
const TYPOS = {
  "gmal.com": "gmail.com", "gmial.com": "gmail.com", "gmai.com": "gmail.com", "gamil.com": "gmail.com",
  "gnail.com": "gmail.com", "gmail.co": "gmail.com", "gmail.con": "gmail.com", "gmail.cm": "gmail.com",
  "hotmial.com": "hotmail.com", "hotmal.com": "hotmail.com", "yahooo.com": "yahoo.com",
  "yaho.com": "yahoo.com", "outlok.com": "outlook.com", "outlook.co": "outlook.com",
  "iclod.com": "icloud.com", "icloud.co": "icloud.com",
};
const PLACEHOLDERS = new Set(["test", "tester", "asdf", "qwerty", "fake", "none", "null", "noreply",
  "no-reply", "nobody", "abc", "user", "name", "email", "mail"]);

/** Why an address that looks right won't reach a person, or null. */
export function notReal(email) {
  const [local, domain] = email.split("@");
  if (TYPOS[domain]) return `Did you mean ${local}@${TYPOS[domain]}?`;
  const base = local.split("+")[0];
  const fake =
    THROWAWAY.has(domain) ||
    RESERVED_TLDS.has(domain.split(".").pop()) ||
    PLACEHOLDERS.has(base.replace(/\d+$/, "")) ||
    base.startsWith("tester") ||
    /^(.)\1+$/.test(base);
  return fake ? "Use an email you read. Fixes and new versions are sent there." : null;
}

/** False only when DNS says the domain takes no mail; a slow or failed lookup lets it through. */
export async function takesMail(domain, lookup = resolveMx) {
  try {
    const mx = await Promise.race([lookup(domain), new Promise((_, no) => setTimeout(() => no(new Error("slow")), 1500))]);
    return Array.isArray(mx) && mx.length > 0;
  } catch (e) {
    return !(e && (e.code === "ENOTFOUND" || e.code === "ENODATA"));
  }
}

let tableReady = false;

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
  if (Number(req.headers["content-length"] ?? 0) > MAX_BODY) {
    return res.status(413).json({ error: "Too large." });
  }
  const ip =
    String(req.headers["x-forwarded-for"] ?? "")
      .split(",")[0]
      .trim() || "unknown";
  if (limited(ip)) {
    res.setHeader("Retry-After", "600");
    return res.status(429).json({ error: "Too many signups. Try again later." });
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
  const why = notReal(s.email);
  if (why) return res.status(422).json({ error: why });
  if (!(await takesMail(s.email.split("@")[1]))) {
    return res.status(422).json({ error: "That email's domain doesn't take mail. Check the spelling." });
  }

  try {
    const sql = neon(url);
    if (!tableReady) {
      await sql`CREATE TABLE IF NOT EXISTS signups (
      email text PRIMARY KEY,
      app_version text,
      os_version text,
      arch text,
      first_seen timestamptz NOT NULL DEFAULT now(),
      last_seen timestamptz NOT NULL DEFAULT now(),
      times integer NOT NULL DEFAULT 1
    )`;
      tableReady = true;
    }
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
