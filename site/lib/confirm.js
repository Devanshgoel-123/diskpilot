// Double opt-in for signups: an address only counts once its owner clicks the
// link sent to it. Shared by api/signup.js and api/confirm.js.
import { createHash, randomBytes } from "node:crypto";

export const SITE = "https://diskpilot.live";
/** A link works for this long. */
export const TOKEN_HOURS = 48;
/** Addresses never confirmed are deleted after this many days. */
export const UNCONFIRMED_DAYS = 30;
/** An address gets at most one confirmation email per this many hours. */
export const RESEND_HOURS = 24;
const TOKEN = /^[A-Za-z0-9_-]{43}$/;

export const newToken = () => randomBytes(32).toString("base64url");
export const hashToken = (t) => createHash("sha256").update(t).digest("hex");
export const validToken = (t) => typeof t === "string" && TOKEN.test(t);

/** Columns the double opt-in needs; safe to run on every cold start. */
export async function migrate(sql) {
  await sql`ALTER TABLE signups
    ADD COLUMN IF NOT EXISTS confirmed_at timestamptz,
    ADD COLUMN IF NOT EXISTS confirm_token_hash text,
    ADD COLUMN IF NOT EXISTS confirm_sent_at timestamptz`;
  // Addresses nobody confirmed are not kept.
  await sql`DELETE FROM signups
    WHERE confirmed_at IS NULL AND first_seen < now() - make_interval(days => ${UNCONFIRMED_DAYS})`;
}

export function confirmationEmail(link) {
  const text = [
    "Hi,",
    "",
    "Someone, hopefully you, entered this address in DiskPilot, the free Mac disk app.",
    "Confirm it to hear about fixes and new versions:",
    "",
    link,
    "",
    `The link works for ${TOKEN_HOURS / 24} days. If this wasn't you, ignore this email: the address won't be used.`,
    "To stop all email from DiskPilot at any time, reply with \"stop\".",
    "",
    "Devansh, DiskPilot",
  ].join("\n");
  const html = `<p>Hi,</p>
<p>Someone, hopefully you, entered this address in DiskPilot, the free Mac disk app. Confirm it to hear about fixes and new versions:</p>
<p><a href="${link}" style="display:inline-block;padding:10px 18px;border-radius:8px;background:#5b55e0;color:#fff;text-decoration:none;font-weight:600">Confirm my email</a></p>
<p style="color:#5d5e6c">The link works for ${TOKEN_HOURS / 24} days. If this wasn't you, ignore this email: the address won't be used. To stop all email from DiskPilot at any time, reply with "stop".</p>
<p>Devansh, DiskPilot</p>`;
  return { subject: "Confirm your email for DiskPilot", text, html };
}

/** Send through Resend's HTTP API. Returns false (and sends nothing) when not configured. */
export async function sendWithResend(to, message, env = process.env, fetchImpl = fetch) {
  if (!env.RESEND_API_KEY) return false;
  const res = await fetchImpl("https://api.resend.com/emails", {
    method: "POST",
    headers: { Authorization: `Bearer ${env.RESEND_API_KEY}`, "Content-Type": "application/json" },
    body: JSON.stringify({
      from: env.SIGNUP_FROM ?? "DiskPilot <hello@diskpilot.live>",
      reply_to: env.SIGNUP_REPLY_TO ?? "devanshgoel.dev@gmail.com",
      to: [to],
      ...message,
    }),
    redirect: "error",
  });
  if (!res.ok) throw new Error(`Resend answered ${res.status}`);
  return true;
}

/**
 * After a signup is stored: send a confirmation link unless the address is
 * already confirmed or was sent one recently. Returns what happened.
 */
export async function startConfirmation(sql, email, send = sendWithResend) {
  const rows = await sql`SELECT confirmed_at, confirm_sent_at FROM signups WHERE email = ${email}`;
  const row = rows[0];
  if (!row) return "missing";
  if (row.confirmed_at) return "confirmed";
  if (row.confirm_sent_at && Date.now() - new Date(row.confirm_sent_at).getTime() < RESEND_HOURS * 3600e3) {
    return "recently-sent";
  }
  const token = newToken();
  // Only the hash is stored, so a database leak can't confirm anyone.
  await sql`UPDATE signups SET confirm_token_hash = ${hashToken(token)}, confirm_sent_at = now()
    WHERE email = ${email} AND confirmed_at IS NULL`;
  const sent = await send(email, confirmationEmail(`${SITE}/api/confirm?token=${token}`));
  return sent ? "sent" : "not-configured";
}

/** Confirm the address a token was sent to. True when it worked. */
export async function confirmToken(sql, token) {
  if (!validToken(token)) return false;
  const rows = await sql`UPDATE signups
    SET confirmed_at = now(), confirm_token_hash = NULL
    WHERE confirm_token_hash = ${hashToken(token)}
      AND confirm_sent_at > now() - make_interval(hours => ${TOKEN_HOURS})
    RETURNING 1 AS ok`;
  return rows.length > 0;
}
