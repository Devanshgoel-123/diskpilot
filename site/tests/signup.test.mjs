// node --test site/tests — signup rules and the double opt-in, with a fake database.
import assert from "node:assert/strict";
import { test } from "node:test";

import handler, { notReal, takesMail } from "../api/signup.js";
import confirmHandler from "../api/confirm.js";
import {
  confirmToken,
  confirmationEmail,
  hashToken,
  newToken,
  sendWithResend,
  startConfirmation,
  validToken,
} from "../lib/confirm.js";

/** A tiny stand-in for the Neon tagged template, backed by one row per email. */
function fakeDb() {
  const rows = new Map();
  const sql = (strings, ...values) => {
    const q = strings.join("?").replace(/\s+/g, " ");
    if (q.startsWith("SELECT confirmed_at")) {
      const r = rows.get(values[0]);
      return Promise.resolve(r ? [r] : []);
    }
    if (q.startsWith("UPDATE signups SET confirm_token_hash")) {
      const r = rows.get(values[1]);
      if (r && !r.confirmed_at) Object.assign(r, { confirm_token_hash: values[0], confirm_sent_at: new Date() });
      return Promise.resolve([]);
    }
    if (q.startsWith("UPDATE signups SET confirmed_at")) {
      for (const r of rows.values()) {
        if (r.confirm_token_hash === values[0]) {
          Object.assign(r, { confirmed_at: new Date(), confirm_token_hash: null });
          return Promise.resolve([{ ok: 1 }]);
        }
      }
      return Promise.resolve([]);
    }
    return Promise.resolve([]);
  };
  return { sql, rows };
}

test("confirmation links: random, only the hash is stored, used once", async () => {
  const { sql, rows } = fakeDb();
  rows.set("a@b.co", { confirmed_at: null, confirm_sent_at: null });
  let mail;
  const sent = await startConfirmation(sql, "a@b.co", async (to, m) => ((mail = { to, ...m }), true));
  assert.equal(sent, "sent");
  assert.equal(mail.to, "a@b.co");
  const token = mail.text.match(/token=([A-Za-z0-9_-]{43})/)[1];
  assert.ok(validToken(token));
  assert.equal(rows.get("a@b.co").confirm_token_hash, hashToken(token));
  assert.ok(!JSON.stringify([...rows.values()]).includes(token), "the token itself is never stored");

  // A second signup within a day sends nothing more.
  assert.equal(await startConfirmation(sql, "a@b.co", async () => assert.fail("no second email")), "recently-sent");

  assert.equal(await confirmToken(sql, token), true);
  assert.ok(rows.get("a@b.co").confirmed_at);
  assert.equal(await confirmToken(sql, token), false, "a link works once");
  assert.equal(await confirmToken(sql, "not-a-token"), false);
  assert.equal(await startConfirmation(sql, "a@b.co", async () => assert.fail("confirmed")), "confirmed");
});

test("without an email provider nothing is sent and nothing fails", async () => {
  assert.equal(await sendWithResend("a@b.co", { subject: "x" }, {}), false);
  const calls = [];
  const ok = await sendWithResend("a@b.co", confirmationEmail("https://x"), { RESEND_API_KEY: "k" }, async (url, init) => {
    calls.push({ url, body: JSON.parse(init.body) });
    return { ok: true };
  });
  assert.ok(ok);
  assert.equal(calls[0].url, "https://api.resend.com/emails");
  assert.deepEqual(calls[0].body.to, ["a@b.co"]);
  assert.match(calls[0].body.text, /reply with "stop"/);
  assert.equal(newToken().length, 43);
});

function res() {
  return {
    code: 0, headers: {}, body: null,
    setHeader(k, v) { this.headers[k.toLowerCase()] = v; },
    status(c) { this.code = c; return this; },
    json(b) { this.body = b; return this; },
    send(b) { this.body = b; return this; },
    end() { return this; },
  };
}

test("signup refuses markup, unknown body sizes and browsers", async () => {
  const ua = { "user-agent": "DiskPilot/0.6.6", "content-length": "80", "x-forwarded-for": "1.1.1.1" };
  const post = (body, headers = ua) => handler({ method: "POST", headers, body }, res());
  assert.equal((await post({ email: "x" }, { ...ua, "user-agent": "Mozilla/5.0" })).code, 403);
  assert.equal((await post({ email: "a@b.co" }, { ...ua, "content-length": undefined })).code, 413);
  assert.equal((await post({ email: "a@b.co" }, { ...ua, "transfer-encoding": "chunked" })).code, 413);
  process.env.DATABASE_URL = "postgres://unused";
  assert.equal((await post({ email: "<svg/onload=alert(1)>@gmail.com" }, { ...ua, "x-forwarded-for": "2.2.2.2" })).code, 400);
  assert.equal((await post({ email: "=cmd|' /c calc'!A0@gmail.com" }, { ...ua, "x-forwarded-for": "3.3.3.3" })).code, 400);
  delete process.env.DATABASE_URL;
});

test("address checks", async () => {
  assert.match(notReal("me@gmial.com"), /gmail\.com/);
  assert.ok(notReal("test@example.com"));
  assert.equal(notReal("dev@company.io"), null);
  assert.equal(await takesMail("x.io", async () => [{ exchange: "mx" }]), true);
  assert.equal(await takesMail("x.io", async () => { throw Object.assign(new Error(), { code: "ENOTFOUND" }); }), false);
});

test("the confirm link shows a button; only the POST confirms", async () => {
  const token = newToken();
  const page = await confirmHandler({ method: "GET", query: { token }, headers: {} }, res());
  assert.equal(page.code, 200);
  assert.match(page.body, /method="post"/);
  assert.ok(!/confirmed/i.test(page.body.replace("Confirm my email", "")), "GET alone confirms nothing");
  const bad = await confirmHandler({ method: "GET", query: { token: "<script>" }, headers: {} }, res());
  assert.equal(bad.code, 400);
  assert.ok(!bad.body.includes("<script>"), "the token is never echoed unless valid");
});
