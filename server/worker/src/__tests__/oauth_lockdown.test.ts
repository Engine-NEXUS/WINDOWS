/**
 * Credential-route lockdown: /oauth/*, /apikeys/*, CORS and the OAuth state nonce.
 *
 * Drives the REAL worker fetch handler against the shared in-memory D1. Proves that a
 * caller who only knows (or guesses) a user_id can no longer read, change or overwrite
 * another account's provider tokens.
 */

import worker from "../index";
import { miniD1 } from "./mini_d1";

const ADMIN = "admin-secret-token";

function kvFake() {
  const m = new Map<string, string>();
  return {
    store: m,
    get: async (k: string) => m.get(k) ?? null,
    put: async (k: string, v: string) => { m.set(k, v); },
    delete: async (k: string) => { m.delete(k); },
  };
}

function makeEnv() {
  const db = miniD1();
  const kv = kvFake();
  const env: any = {
    DB: db as any,
    AI: { run: async () => ({ response: "ok" }) },
    CACHE: kv as any,
    MIGRATION_MODE: "strict",
    NEXUS_ADMIN_IDENTITY_TOKEN: ADMIN,
    NEXUS_ADMIN_TOKEN: "publish-secret",
    GOOGLE_CLIENT_ID: "gid",
    GOOGLE_CLIENT_SECRET: "gsecret",
    GITHUB_CLIENT_ID: "ghid",
    GITHUB_CLIENT_SECRET: "ghsecret",
  };
  return { env, db, kv };
}

function req(url: string, method = "GET", body?: unknown, headers: Record<string, string> = {}) {
  return new Request(url, {
    method,
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

/** Claim a device and (optionally) have the admin approve it. Returns auth headers. */
async function enroll(env: any, approve = true, hint = "user_enroll") {
  const claim = await (await worker.fetch(req("https://w.test/v1/profiles/claim", "POST", {
    provisional_user_id: hint, device_name: "Laptop", os: "windows",
  }), env)).json() as any;
  if (approve) {
    const ap = await worker.fetch(req("https://w.test/v1/admin/profiles/approve", "POST", {
      profile_id: claim.profile_id, quota_tier: "default",
    }, { Authorization: `Bearer ${ADMIN}` }), env);
    expect(ap.status).toBe(200);
  }
  return {
    profileId: claim.profile_id as string,
    headers: {
      Authorization: `Bearer ${claim.device_token}`,
      "X-Nexus-Profile-Id": claim.profile_id,
      "X-Nexus-Device-Id": claim.device_id,
    } as Record<string, string>,
  };
}

const FAR_FUTURE = 9_999_999_999;
function seedToken(db: any, ownerKey: string, provider: string, token: string) {
  db._t.oauth_tokens.set(`${ownerKey}:${provider}`, {
    user_id: ownerKey, provider, access_token: token, refresh_token: null, expires_at: FAR_FUTURE,
  });
}

describe("credential routes require an approved device", () => {
  const routes: Array<[string, string, unknown?]> = [
    ["GET", "/oauth/google-token?user_id=victim"],
    ["GET", "/oauth/github-token?user_id=victim"],
    ["GET", "/oauth/swiggy-token?user_id=victim"],
    ["GET", "/oauth/status?user_id=victim"],
    ["GET", "/oauth/auth-url?provider=google&user_id=victim"],
    ["POST", "/oauth/exchange", { provider: "google", code: "c", user_id: "victim" }],
    ["DELETE", "/oauth/disconnect", { provider: "google", user_id: "victim" }],
    ["GET", "/apikeys/list?user_id=victim"],
    ["DELETE", "/apikeys/remove", { provider: "groq", user_id: "victim" }],
    ["POST", "/apikeys/add", { provider: "groq", api_key: "k", user_id: "victim" }],
    ["GET", "/oauth/unknown-route"],
  ];

  test.each(routes)("%s %s without credentials → 401", async (method, path, body) => {
    const { env, db } = makeEnv();
    seedToken(db, "victim", "google", "VICTIM-GOOGLE-TOKEN");
    const res = await worker.fetch(req(`https://w.test${path}`, method, body), env);
    expect(res.status).toBe(401);
    expect(await res.text()).not.toContain("VICTIM");
  });

  test("a valid user_id alone is not enough (the original hole)", async () => {
    const { env, db } = makeEnv();
    seedToken(db, "victim", "google", "VICTIM-GOOGLE-TOKEN");
    const res = await worker.fetch(req("https://w.test/oauth/google-token?user_id=victim"), env);
    expect(res.status).toBe(401);
  });

  test("wrong bearer token → 401", async () => {
    const { env } = makeEnv();
    const dev = await enroll(env);
    const res = await worker.fetch(req("https://w.test/oauth/status", "GET", undefined, {
      ...dev.headers, Authorization: "Bearer not-the-token",
    }), env);
    expect(res.status).toBe(401);
  });

  test("forged profile id with someone else's device token → 401", async () => {
    const { env } = makeEnv();
    const a = await enroll(env, true, "user_a");
    const b = await enroll(env, true, "user_b");
    const res = await worker.fetch(req("https://w.test/oauth/status", "GET", undefined, {
      ...a.headers, "X-Nexus-Profile-Id": b.profileId,
    }), env);
    expect(res.status).toBe(401);
  });

  test("a pending (unapproved) device is refused", async () => {
    const { env } = makeEnv();
    const dev = await enroll(env, false);
    const res = await worker.fetch(req("https://w.test/oauth/status", "GET", undefined, dev.headers), env);
    expect(res.status).toBe(403);
  });
});

describe("authenticated callers only reach their own data", () => {
  test("own token is returned; naming another user's id is 403 and leaks nothing", async () => {
    const { env, db } = makeEnv();
    const a = await enroll(env, true, "user_a");
    seedToken(db, a.profileId, "google", "A-GOOGLE-TOKEN");
    seedToken(db, "victim", "google", "VICTIM-GOOGLE-TOKEN");

    const own = await worker.fetch(req("https://w.test/oauth/google-token", "GET", undefined, a.headers), env);
    expect(own.status).toBe(200);
    expect((await own.json() as any).token).toBe("A-GOOGLE-TOKEN");

    const sameViaId = await worker.fetch(req(
      `https://w.test/oauth/google-token?user_id=${a.profileId}`, "GET", undefined, a.headers), env);
    expect(sameViaId.status).toBe(200);

    const forged = await worker.fetch(req(
      "https://w.test/oauth/google-token?user_id=victim", "GET", undefined, a.headers), env);
    expect(forged.status).toBe(403);
    expect(await forged.text()).not.toContain("VICTIM");
  });

  test("a legacy id migrated to this profile stays reachable", async () => {
    const { env, db } = makeEnv();
    const a = await enroll(env, true, "user_a");
    db._t.identity_migration.set("legacy_a", { legacy_user_id: "legacy_a", profile_id: a.profileId });
    seedToken(db, "legacy_a", "github", "LEGACY-GH-TOKEN");
    const res = await worker.fetch(req(
      "https://w.test/oauth/github-token?user_id=legacy_a", "GET", undefined, a.headers), env);
    expect(res.status).toBe(200);
    expect((await res.json() as any).token).toBe("LEGACY-GH-TOKEN");
  });

  test("another profile's migrated legacy id is refused", async () => {
    const { env, db } = makeEnv();
    const a = await enroll(env, true, "user_a");
    const b = await enroll(env, true, "user_b");
    db._t.identity_migration.set("legacy_b", { legacy_user_id: "legacy_b", profile_id: b.profileId });
    seedToken(db, "legacy_b", "github", "B-GH-TOKEN");
    const res = await worker.fetch(req(
      "https://w.test/oauth/github-token?user_id=legacy_b", "GET", undefined, a.headers), env);
    expect(res.status).toBe(403);
  });

  test("disconnect cannot delete another user's connection", async () => {
    const { env, db } = makeEnv();
    const a = await enroll(env, true, "user_a");
    seedToken(db, "victim", "google", "VICTIM-GOOGLE-TOKEN");
    const res = await worker.fetch(req("https://w.test/oauth/disconnect", "DELETE",
      { provider: "google", user_id: "victim" }, a.headers), env);
    expect(res.status).toBe(403);
    expect(db._t.oauth_tokens.has("victim:google")).toBe(true);
  });

  test("storing API keys server-side is gone (410), even when authenticated", async () => {
    const { env } = makeEnv();
    const a = await enroll(env, true, "user_a");
    const res = await worker.fetch(req("https://w.test/apikeys/add", "POST",
      { provider: "groq", api_key: "gsk_secret" }, a.headers), env);
    expect(res.status).toBe(410);
  });
});

describe("OAuth state nonce and callback", () => {
  async function startFlow(env: any, headers: Record<string, string>, provider = "google") {
    const res = await worker.fetch(req(
      `https://w.test/oauth/auth-url?provider=${provider}`, "GET", undefined, headers), env);
    expect(res.status).toBe(200);
    const { url } = await res.json() as any;
    return new URL(url).searchParams.get("state") as string;
  }

  test("auth-url issues an unguessable single-use state bound to the owner", async () => {
    const { env, kv } = makeEnv();
    const a = await enroll(env);
    const state = await startFlow(env, a.headers);
    expect(state).toMatch(/^[0-9a-f]{48}$/);
    const rec = JSON.parse(kv.store.get(`oauth_state:${state}`) as string);
    expect(rec).toEqual({ provider: "google", ownerKey: a.profileId });
  });

  test("auth-url refuses an unsupported provider and a missing KV binding", async () => {
    const { env } = makeEnv();
    const a = await enroll(env);
    const bad = await worker.fetch(req("https://w.test/oauth/auth-url?provider=evil", "GET", undefined, a.headers), env);
    expect(bad.status).toBe(400);
    const noKv = { ...env, CACHE: undefined };
    const res = await worker.fetch(req("https://w.test/oauth/auth-url?provider=google", "GET", undefined, a.headers), noKv);
    expect(res.status).toBe(503);
  });

  test("callback rejects the old forgeable 'provider:userId' state and writes nothing", async () => {
    const { env, db } = makeEnv();
    const res = await worker.fetch(req("https://w.test/oauth/callback?code=abc&state=google:victim"), env);
    expect(res.status).toBe(400);
    expect(db._t.oauth_tokens.size).toBe(0);
  });

  test("a state is burned after first use", async () => {
    const { env } = makeEnv();
    const a = await enroll(env);
    const state = await startFlow(env, a.headers);
    const first = await worker.fetch(req(`https://w.test/oauth/callback?error=access_denied&state=${state}`), env);
    expect(first.status).toBe(200);
    const replay = await worker.fetch(req(`https://w.test/oauth/callback?error=access_denied&state=${state}`), env);
    expect(replay.status).toBe(400);
  });

  test("provider error text is HTML-escaped (no reflected XSS)", async () => {
    const { env } = makeEnv();
    const a = await enroll(env);
    const state = await startFlow(env, a.headers);
    const payload = encodeURIComponent('<script>alert(1)</script><img src=x onerror=alert(2)>');
    const res = await worker.fetch(req(`https://w.test/oauth/callback?error=${payload}&state=${state}`), env);
    const html = await res.text();
    expect(html).not.toContain("<script>alert(1)");
    expect(html).not.toContain("<img src=x");
    expect(html).toContain("&lt;script&gt;");
  });

  test("an invalid-state page does not reflect attacker input either", async () => {
    const { env } = makeEnv();
    const payload = encodeURIComponent('"><script>alert(1)</script>');
    const res = await worker.fetch(req(`https://w.test/oauth/callback?error=${payload}&state=${payload}`), env);
    expect(await res.text()).not.toContain("<script>alert(1)");
  });
});

describe("CORS and admin publish", () => {
  test("only the desktop WebView origins are echoed; others get no ACAO header", async () => {
    const { env } = makeEnv();
    const evil = await worker.fetch(req("https://w.test/health", "GET", undefined, { Origin: "https://evil.example" }), env);
    expect(evil.headers.get("Access-Control-Allow-Origin")).toBeNull();
    const app = await worker.fetch(req("https://w.test/health", "GET", undefined, { Origin: "http://tauri.localhost" }), env);
    expect(app.headers.get("Access-Control-Allow-Origin")).toBe("http://tauri.localhost");
    const star = await worker.fetch(req("https://w.test/health", "GET", undefined, { Origin: "http://tauri.localhost" }), env);
    expect(star.headers.get("Access-Control-Allow-Origin")).not.toBe("*");
  });

  test("preflight allows the identity headers for the app origin only", async () => {
    const { env } = makeEnv();
    const ok = await worker.fetch(new Request("https://w.test/oauth/status", {
      method: "OPTIONS", headers: { Origin: "http://tauri.localhost" },
    }), env);
    expect(ok.headers.get("Access-Control-Allow-Headers")).toContain("X-Nexus-Profile-Id");
    expect(ok.headers.get("Access-Control-Allow-Origin")).toBe("http://tauri.localhost");
    const bad = await worker.fetch(new Request("https://w.test/oauth/status", {
      method: "OPTIONS", headers: { Origin: "https://evil.example" },
    }), env);
    expect(bad.headers.get("Access-Control-Allow-Origin")).toBeNull();
  });

  test("model publish needs the exact admin token", async () => {
    const { env } = makeEnv();
    const body = { version: "1", files: { "labels.json": { sha256: "x", size: 1 } } };
    const wrong = await worker.fetch(req("https://w.test/models/nlu/publish", "POST", body,
      { Authorization: "Bearer publish-secre" }), env);
    expect(wrong.status).toBe(401);
    const ok = await worker.fetch(req("https://w.test/models/nlu/publish", "POST", body,
      { Authorization: "Bearer publish-secret" }), env);
    expect(ok.status).toBe(200);
  });
});
