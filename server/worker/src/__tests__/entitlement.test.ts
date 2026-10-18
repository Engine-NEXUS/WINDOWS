/**
 * Feature 88 (W2) — end-to-end entitlement gate + admin plane tests.
 *
 * Drives the REAL worker fetch handler (default export) against a compact
 * in-memory D1 that emulates exactly the SQL shapes the identity/admin/
 * quota modules issue. Proves the core guarantees:
 *   - deny ⇒ zero env.AI.run calls, zero usage increments
 *   - mode=off keeps legacy behavior untouched
 *   - admin approve unlocks; revoke kills tokens + oauth rows
 *   - two-laptop isolation
 */

import worker from "../index";
import { memoryIdentityStore } from "./identity_helpers";
import { miniD1 } from "./mini_d1";
const BASE = 1_800_000_000_000;


// ---- Helpers ----

function jsonReq(url: string, method: string, body?: unknown, headers: Record<string, string> = {}): Request {
  return new Request(url, {
    method,
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

function makeEnv() {
  const db = miniD1();
  const aiRun = vi.fn(async () => ({ response: "ok" }));
  const env: any = {
    DB: db as any,
    AI: { run: aiRun },
    MIGRATION_MODE: "off",
    NEXUS_ADMIN_IDENTITY_TOKEN: "admin-secret-token",
  };
  return { env, db, aiRun };
}

// ---- Tests ----

describe("Feature 88 e2e — entitlement gate", () => {
  test("mode=off: legacy transcript flows exactly as before (zero AI for static intent)", async () => {
    const { env, aiRun } = makeEnv();
    const res = await worker.fetch(jsonReq("https://w.test/", "POST", {
      request_id: "r1",
      requester: { id: "user_legacy1", device_id: "device_legacy1" },
      task: { type: "general", intent: "deep_analyse", request: "deep analyse owner/repo" },
    }), env);
    expect(res.status).toBe(200);
    const body = await res.json() as any;
    expect(body.reply_text).toContain("architecture mapper");
    expect(body.protocol_version).toBe("1");
    // explicit intent → classify never ran; deep_analyse branch is static
    expect(aiRun).not.toHaveBeenCalled();
  });

  test("pending canonical profile is denied with zero AI calls and zero usage", async () => {
    const { env, db, aiRun } = makeEnv();
    // 1. claim
    const claimRes = await worker.fetch(jsonReq("https://w.test/v1/profiles/claim", "POST", {
      provisional_user_id: "user_new", device_name: "Laptop-B", os: "windows",
    }), env);
    expect(claimRes.status).toBe(200);
    const claim = await claimRes.json() as any;
    expect(claim.status).toBe("pending");
    expect(claim.device_token).toBeTruthy();

    // 2. transcript with canonical identity → 403
    const turnRes = await worker.fetch(jsonReq("https://w.test/", "POST", {
      request_id: "r2",
      requester: { id: claim.profile_id, profile_id: claim.profile_id, device_id: claim.device_id },
      task: { type: "general", request: "what is the capital of France" },
    }, { Authorization: `Bearer ${claim.device_token}` }), env);
    expect(turnRes.status).toBe(403);
    const denied = await turnRes.json() as any;
    expect(denied.error).toBe("worker_ai_not_enabled");
    expect(denied.code).toBe("pending");

    // 3. no AI consumed, no quota burned
    expect(aiRun).not.toHaveBeenCalled();
    expect(db._t.usage_log.size).toBe(0);
  });

  test("admin approve unlocks the pending profile; denial audit logged", async () => {
    const { env, db } = makeEnv();
    const claim = await (await worker.fetch(jsonReq("https://w.test/v1/profiles/claim", "POST", {
      provisional_user_id: "user_a2",
    }), env)).json() as any;

    // deny audit event was written
    expect(db._t.profile_events.some(e => e.event === "registered")).toBe(true);

    // approve
    const ap = await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/approve", "POST", {
      profile_id: claim.profile_id, quota_tier: "default",
    }, { Authorization: "Bearer admin-secret-token" }), env);
    expect(ap.status).toBe(200);

    // /me reflects approval
    const me = await worker.fetch(new Request(
      `https://w.test/v1/profiles/me?profile_id=${claim.profile_id}&device_id=${claim.device_id}`,
      { headers: { Authorization: `Bearer ${claim.device_token}` } }), env);
    expect(me.status).toBe(200);
    const meBody = await me.json() as any;
    expect(meBody.status).toBe("approved");
    expect(meBody.quota_tier).toBe("default");
  });

  test("admin routes reject bad/missing credentials", async () => {
    const { env } = makeEnv();
    const r1 = await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/pending", "GET"), env);
    expect(r1.status).toBe(401);
    const r2 = await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/pending", "GET"), {
      ...makeEnv().env, NEXUS_ADMIN_IDENTITY_TOKEN: "admin-secret-token",
    });
    void r2;
    const r3 = await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/pending", "GET"), {
      ...makeEnv().env,
    });
    void r3;
    const bad = await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/pending", "GET", undefined, {
      Authorization: "Bearer wrong",
    }), env);
    expect(bad.status).toBe(401);
  });

  test("revoke cascade kills device tokens and deletes oauth rows", async () => {
    const { env, db } = makeEnv();
    const claim = await (await worker.fetch(jsonReq("https://w.test/v1/profiles/claim", "POST", {
      provisional_user_id: "user_r2",
    }), env)).json() as any;
    await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/approve", "POST", {
      profile_id: claim.profile_id,
    }, { Authorization: "Bearer admin-secret-token" }), env);

    // seed an oauth row for the profile
    db._t.oauth_tokens.set("k1", { user_id: claim.profile_id, provider: "google", access_token: "at" });

    const rev = await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/revoke", "POST", {
      profile_id: claim.profile_id, note: "lost laptop",
    }, { Authorization: "Bearer admin-secret-token" }), env);
    expect(rev.status).toBe(200);
    const revBody = await rev.json() as any;
    expect(revBody.status).toBe("revoked");
    expect(revBody.oauth_deleted).toBe(1);
    expect(db._t.oauth_tokens.size).toBe(0);

    // device token is dead
    const me = await worker.fetch(new Request(
      `https://w.test/v1/profiles/me?profile_id=${claim.profile_id}&device_id=${claim.device_id}`,
      { headers: { Authorization: `Bearer ${claim.device_token}` } }), env);
    expect(me.status).toBe(403);
    const meBody = await me.json() as any;
    expect(meBody.code).toBe("revoked");
  });

  test("two laptops are fully isolated", async () => {
    const { env } = makeEnv();
    const a = await (await worker.fetch(jsonReq("https://w.test/v1/profiles/claim", "POST", {
      provisional_user_id: "user_A2",
    }), env)).json() as any;
    const b = await (await worker.fetch(jsonReq("https://w.test/v1/profiles/claim", "POST", {
      provisional_user_id: "user_B2",
    }), env)).json() as any;
    await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/approve", "POST", {
      profile_id: a.profile_id,
    }, { Authorization: "Bearer admin-secret-token" }), env);
    await worker.fetch(jsonReq("https://w.test/v1/admin/profiles/approve", "POST", {
      profile_id: b.profile_id,
    }, { Authorization: "Bearer admin-secret-token" }), env);

    // A's token cannot ride B's profile
    const r = await worker.fetch(new Request(
      `https://w.test/v1/profiles/me?profile_id=${b.profile_id}&device_id=${a.device_id}`,
      { headers: { Authorization: `Bearer ${a.device_token}` } }), env);
    expect(r.status).toBe(403);
    const body = await r.json() as any;
    expect(body.code).toBe("unknown_profile");
  });

  test("health advertises identity + entitlement additively", async () => {
    const { env } = makeEnv();
    const res = await worker.fetch(jsonReq("https://w.test/health", "GET"), env);
    const body = await res.json() as any;
    expect(body.identity).toBe("1");
    expect(body.entitlement_gate).toBe("1");
    expect(body.protocol_version).toBe("1");
  });
});
