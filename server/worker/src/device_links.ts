/** Device Links control plane. No file bytes or activity history enter D1. */
import { sha256Hex, timingSafeEqualHex } from "./identity";
import { timingSafeEqualStr } from "./route_auth";

export interface LinkEnv {
  DB: D1Database;
  LINK_SIGNAL?: DurableObjectNamespace;
  NEXUS_LINK_CODE_PEPPER?: string;
  NEXUS_SMS_GATEWAY_TOKEN?: string;
  NEXUS_SMS_CODE_KEY?: string;
}
export interface LinkCaller { profileId: string; deviceId: string }

const PHONE_TTL_MS = 10 * 60_000;
const LINK_TTL_MS = 10 * 60_000;
const MAX_ATTEMPTS = 5;

/** Daily cleanup of expired proof rows; quota errors defer the next pass. */
export async function cleanupLinkProofs(db: D1Database, now = Date.now()): Promise<void> {
  await db.prepare("DELETE FROM link_phone_challenges WHERE expires_at < ?").bind(now).run();
  await db.prepare("DELETE FROM device_link_requests WHERE expires_at < ?").bind(now).run();
  await db.prepare("DELETE FROM device_links WHERE revoked_at IS NOT NULL AND revoked_at < ?")
    .bind(now - 30 * 86_400_000).run();
}

export function validPhone(value: unknown): value is string {
  return typeof value === "string" && /^\+[1-9][0-9]{7,14}$/.test(value);
}

export function orderedDevices(a: string, b: string): [string, string] {
  return a < b ? [a, b] : [b, a];
}

function code(): string {
  const bytes = new Uint32Array(1);
  crypto.getRandomValues(bytes);
  return String(bytes[0] % 1_000_000).padStart(6, "0");
}

function configured(env: LinkEnv): boolean {
  return !!(env.NEXUS_LINK_CODE_PEPPER && env.NEXUS_SMS_GATEWAY_TOKEN && env.NEXUS_SMS_CODE_KEY);
}

async function codeHash(value: string, pepper: string): Promise<string> {
  return sha256Hex(`${pepper}:${value}`);
}

async function aesKey(raw: string): Promise<CryptoKey> {
  const bytes = Uint8Array.from(atob(raw), c => c.charCodeAt(0));
  if (bytes.length !== 32) throw new Error("sms_key_invalid");
  return crypto.subtle.importKey("raw", bytes, "AES-GCM", false, ["encrypt", "decrypt"]);
}

async function encryptCode(value: string, rawKey: string): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const cipher = new Uint8Array(await crypto.subtle.encrypt(
    { name: "AES-GCM", iv }, await aesKey(rawKey), new TextEncoder().encode(value),
  ));
  return btoa(String.fromCharCode(...iv, ...cipher));
}

async function decryptCode(value: string, rawKey: string): Promise<string> {
  const bytes = Uint8Array.from(atob(value), c => c.charCodeAt(0));
  const plaintext = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: bytes.slice(0, 12) }, await aesKey(rawKey), bytes.slice(12),
  );
  return new TextDecoder().decode(plaintext);
}

async function bodyOf(request: Request): Promise<Record<string, unknown> | null> {
  try {
    const body = await request.json();
    return body && typeof body === "object" && !Array.isArray(body)
      ? body as Record<string, unknown> : null;
  } catch { return null; }
}

function reply(data: unknown, status = 200, cors: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(data), {
    status, headers: { "Content-Type": "application/json", "Cache-Control": "no-store", ...cors },
  });
}

/** An authenticated spare-phone gateway polls this endpoint and acknowledges actual sends. */
export async function handleSmsGateway(
  request: Request, env: LinkEnv, cors: Record<string, string>, now = Date.now(),
): Promise<Response> {
  if (!configured(env)) return reply({ error: "sms_gateway_unavailable" }, 503, cors);
  const supplied = request.headers.get("Authorization") || "";
  if (!timingSafeEqualStr(supplied, `Bearer ${env.NEXUS_SMS_GATEWAY_TOKEN}`)) {
    return reply({ error: "gateway_auth_required" }, 401, cors);
  }
  const path = new URL(request.url).pathname;
  if (path === "/v1/links/sms/heartbeat" && request.method === "POST") {
    await env.DB.prepare("INSERT INTO link_sms_gateway_state (singleton, last_seen_at) VALUES (1, ?) ON CONFLICT(singleton) DO UPDATE SET last_seen_at = excluded.last_seen_at")
      .bind(now).run();
    return reply({ ready: true }, 200, cors);
  }
  if (path === "/v1/links/sms/next" && request.method === "POST") {
    // One atomic claim prevents concurrent gateway polls from receiving the
    // same code. A stale claim becomes eligible again after 60 seconds.
    const row = await env.DB.prepare(
      "UPDATE link_phone_challenges SET claimed_at = ? WHERE challenge_id = (SELECT challenge_id FROM link_phone_challenges WHERE sent_at IS NULL AND consumed_at IS NULL AND expires_at > ? AND (claimed_at IS NULL OR claimed_at < ?) ORDER BY created_at LIMIT 1) AND sent_at IS NULL AND consumed_at IS NULL AND expires_at > ? RETURNING challenge_id, phone, encrypted_code"
    ).bind(now, now, now - 60_000, now).first<{ challenge_id: string; phone: string; encrypted_code: string }>();
    if (!row) return reply({ pending: false }, 200, cors);
    return reply({ pending: true, challenge_id: row.challenge_id, phone: row.phone,
      code: await decryptCode(row.encrypted_code, env.NEXUS_SMS_CODE_KEY!) }, 200, cors);
  }
  if (path === "/v1/links/sms/sent" && request.method === "POST") {
    const body = await bodyOf(request);
    if (typeof body?.challenge_id !== "string") return reply({ error: "invalid_body" }, 400, cors);
    await env.DB.prepare("UPDATE link_phone_challenges SET sent_at = ?, encrypted_code = '' WHERE challenge_id = ? AND sent_at IS NULL AND expires_at > ?")
      .bind(now, body.challenge_id, now).run();
    return reply({ ok: true }, 200, cors);
  }
  return reply({ error: "not_found" }, 404, cors);
}

export async function handleDeviceLink(
  request: Request, env: LinkEnv, caller: LinkCaller,
  cors: Record<string, string>, now = Date.now(),
): Promise<Response> {
  const path = new URL(request.url).pathname;
  const method = request.method;
  const db = env.DB;
  if (path === "/v1/links/phone/start" && method === "POST") {
    if (!configured(env)) return reply({ error: "sms_gateway_unavailable" }, 503, cors);
    const gateway = await db.prepare("SELECT last_seen_at FROM link_sms_gateway_state WHERE singleton = 1")
      .first<{ last_seen_at: number }>();
    if (!gateway || gateway.last_seen_at < now - 5 * 60_000) {
      return reply({ error: "sms_gateway_offline" }, 503, cors);
    }
    const body = await bodyOf(request);
    if (!validPhone(body?.phone)) return reply({ error: "invalid_phone" }, 400, cors);
    const own = await db.prepare("SELECT phone FROM link_phone_bindings WHERE device_id = ?")
      .bind(caller.deviceId).first<{ phone: string }>();
    if (own && own.phone !== body.phone) return reply({ error: "revoke_existing_phone_first" }, 409, cors);
    if (own) return reply({ error: "phone_already_verified" }, 409, cors);
    const existing = await db.prepare("SELECT device_id FROM link_phone_bindings WHERE phone = ?")
      .bind(body.phone).first<{ device_id: string }>();
    if (existing && existing.device_id !== caller.deviceId) return reply({ error: "phone_in_use" }, 409, cors);
    const count = await db.prepare("SELECT COUNT(*) AS n FROM link_phone_challenges WHERE device_id = ? AND created_at > ?")
      .bind(caller.deviceId, now - 86_400_000).first<{ n: number }>();
    if ((count?.n || 0) >= 3) return reply({ error: "verification_rate_limited" }, 429, cors);
    const challengeId = crypto.randomUUID();
    const pin = code();
    await db.prepare("INSERT INTO link_phone_challenges (challenge_id, device_id, phone, code_hash, encrypted_code, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
      .bind(challengeId, caller.deviceId, body.phone,
        await codeHash(pin, env.NEXUS_LINK_CODE_PEPPER!),
        await encryptCode(pin, env.NEXUS_SMS_CODE_KEY!), now, now + PHONE_TTL_MS).run();
    return reply({ challenge_id: challengeId, expires_at: now + PHONE_TTL_MS,
      status: "waiting_for_sms_gateway" }, 202, cors);
  }
  if (path === "/v1/links/phone/confirm" && method === "POST") {
    if (!env.NEXUS_LINK_CODE_PEPPER) return reply({ error: "links_unavailable" }, 503, cors);
    const body = await bodyOf(request);
    if (typeof body?.challenge_id !== "string" || !/^[0-9]{6}$/.test(String(body?.code))) {
      return reply({ error: "invalid_body" }, 400, cors);
    }
    const row = await db.prepare("SELECT phone, code_hash, expires_at, sent_at, attempts, consumed_at FROM link_phone_challenges WHERE challenge_id = ? AND device_id = ?")
      .bind(body.challenge_id, caller.deviceId).first<{ phone: string; code_hash: string; expires_at: number; sent_at: number | null; attempts: number; consumed_at: number | null }>();
    if (!row || row.expires_at <= now || row.consumed_at || row.attempts >= MAX_ATTEMPTS || !row.sent_at) {
      return reply({ error: "challenge_unavailable" }, 403, cors);
    }
    const attempted = await db.prepare("UPDATE link_phone_challenges SET attempts = attempts + 1 WHERE challenge_id = ? AND device_id = ? AND attempts < ? AND consumed_at IS NULL AND sent_at IS NOT NULL AND expires_at > ?")
      .bind(body.challenge_id, caller.deviceId, MAX_ATTEMPTS, now).run();
    if ((attempted.meta.changes || 0) !== 1) return reply({ error: "challenge_unavailable" }, 403, cors);
    if (!timingSafeEqualHex(await codeHash(String(body.code), env.NEXUS_LINK_CODE_PEPPER || ""), row.code_hash)) {
      return reply({ error: "invalid_code" }, 403, cors);
    }
    const bound = await db.prepare("SELECT device_id FROM link_phone_bindings WHERE phone = ?")
      .bind(row.phone).first<{ device_id: string }>();
    if (bound && bound.device_id !== caller.deviceId) return reply({ error: "phone_in_use" }, 409, cors);
    const own = await db.prepare("SELECT phone FROM link_phone_bindings WHERE device_id = ?")
      .bind(caller.deviceId).first<{ phone: string }>();
    if (own && own.phone !== row.phone) return reply({ error: "revoke_existing_phone_first" }, 409, cors);
    const consumed = await db.prepare("UPDATE link_phone_challenges SET consumed_at = ?, encrypted_code = '' WHERE challenge_id = ? AND device_id = ? AND consumed_at IS NULL AND sent_at IS NOT NULL AND expires_at > ?")
      .bind(now, body.challenge_id, caller.deviceId, now).run();
    if ((consumed.meta.changes || 0) !== 1) return reply({ error: "challenge_unavailable" }, 403, cors);
    await db.prepare("INSERT INTO link_phone_bindings (device_id, phone, verified_at) VALUES (?, ?, ?) ON CONFLICT(device_id) DO UPDATE SET verified_at = excluded.verified_at")
      .bind(caller.deviceId, row.phone, now).run();
    return reply({ verified: true, phone: row.phone }, 200, cors);
  }
  if (path === "/v1/links/phone" && method === "GET") {
    const row = await db.prepare("SELECT phone, verified_at FROM link_phone_bindings WHERE device_id = ?")
      .bind(caller.deviceId).first();
    return reply({ binding: row || null }, 200, cors);
  }
  if (path === "/v1/links/phone" && method === "DELETE") {
    const result = await db.prepare("DELETE FROM link_phone_bindings WHERE device_id = ?")
      .bind(caller.deviceId).run();
    return reply({ revoked: (result.meta.changes || 0) === 1 }, 200, cors);
  }
  if (path === "/v1/links/request" && method === "POST") {
    if (!env.NEXUS_LINK_CODE_PEPPER) return reply({ error: "links_unavailable" }, 503, cors);
    const body = await bodyOf(request);
    if (!validPhone(body?.phone)) return reply({ error: "invalid_phone" }, 400, cors);
    const own = await db.prepare("SELECT phone FROM link_phone_bindings WHERE device_id = ?")
      .bind(caller.deviceId).first();
    if (!own) return reply({ error: "own_phone_unverified" }, 403, cors);
    const target = await db.prepare("SELECT b.device_id FROM link_phone_bindings b JOIN devices d ON d.device_id = b.device_id WHERE b.phone = ? AND d.status = 'active'")
      .bind(body.phone).first<{ device_id: string }>();
    if (!target) return reply({ error: "target_unavailable" }, 404, cors);
    if (target.device_id === caller.deviceId) return reply({ error: "self_link" }, 400, cors);
    const [a, b] = orderedDevices(caller.deviceId, target.device_id);
    const linked = await db.prepare("SELECT link_id FROM device_links WHERE device_a = ? AND device_b = ? AND revoked_at IS NULL")
      .bind(a, b).first();
    if (linked) return reply({ error: "already_linked" }, 409, cors);
    const count = await db.prepare("SELECT COUNT(*) AS n FROM device_link_requests WHERE source_device_id = ? AND created_at > ?")
      .bind(caller.deviceId, now - 86_400_000).first<{ n: number }>();
    if ((count?.n || 0) >= 10) return reply({ error: "link_rate_limited" }, 429, cors);
    const requestId = crypto.randomUUID();
    const pin = code();
    await db.prepare("INSERT INTO device_link_requests (request_id, source_device_id, target_device_id, code_hash, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)")
      .bind(requestId, caller.deviceId, target.device_id,
        await codeHash(pin, env.NEXUS_LINK_CODE_PEPPER || ""), now, now + LINK_TTL_MS).run();
    return reply({ request_id: requestId, code: pin, expires_at: now + LINK_TTL_MS }, 201, cors);
  }
  if (path === "/v1/links/pending" && method === "GET") {
    const pending = await db.prepare("SELECT r.request_id, r.source_device_id, d.device_name, r.expires_at FROM device_link_requests r JOIN devices d ON d.device_id = r.source_device_id WHERE r.target_device_id = ? AND r.accepted_at IS NULL AND r.rejected_at IS NULL AND r.expires_at > ? ORDER BY r.created_at DESC LIMIT 20")
      .bind(caller.deviceId, now).all();
    return reply({ requests: pending.results || [] }, 200, cors);
  }
  if (path === "/v1/links/accept" && method === "POST") {
    if (!env.NEXUS_LINK_CODE_PEPPER) return reply({ error: "links_unavailable" }, 503, cors);
    const body = await bodyOf(request);
    if (typeof body?.request_id !== "string" || !/^[0-9]{6}$/.test(String(body?.code))) {
      return reply({ error: "invalid_body" }, 400, cors);
    }
    const row = await db.prepare("SELECT source_device_id, code_hash, expires_at, attempts, accepted_at, rejected_at FROM device_link_requests WHERE request_id = ? AND target_device_id = ?")
      .bind(body.request_id, caller.deviceId).first<{ source_device_id: string; code_hash: string; expires_at: number; attempts: number; accepted_at: number | null; rejected_at: number | null }>();
    if (!row || row.expires_at <= now || row.accepted_at || row.rejected_at || row.attempts >= MAX_ATTEMPTS) {
      return reply({ error: "request_unavailable" }, 403, cors);
    }
    const source = await db.prepare("SELECT status FROM devices WHERE device_id = ?")
      .bind(row.source_device_id).first<{ status: string }>();
    if (source?.status !== "active") return reply({ error: "source_unavailable" }, 403, cors);
    const bound = await db.prepare("SELECT device_id FROM link_phone_bindings WHERE device_id IN (?, ?)").bind(row.source_device_id, caller.deviceId).all();
    if ((bound.results || []).length !== 2) return reply({ error: "phone_verification_required" }, 403, cors);
    const attempted = await db.prepare("UPDATE device_link_requests SET attempts = attempts + 1 WHERE request_id = ? AND target_device_id = ? AND attempts < ? AND accepted_at IS NULL AND rejected_at IS NULL AND expires_at > ?")
      .bind(body.request_id, caller.deviceId, MAX_ATTEMPTS, now).run();
    if ((attempted.meta.changes || 0) !== 1) return reply({ error: "request_unavailable" }, 403, cors);
    if (!timingSafeEqualHex(await codeHash(String(body.code), env.NEXUS_LINK_CODE_PEPPER || ""), row.code_hash)) {
      return reply({ error: "invalid_code" }, 403, cors);
    }
    const [a, b] = orderedDevices(row.source_device_id, caller.deviceId);
    const linkId = crypto.randomUUID();
    const accepted = await db.prepare("UPDATE device_link_requests SET accepted_at = ? WHERE request_id = ? AND target_device_id = ? AND accepted_at IS NULL AND rejected_at IS NULL AND expires_at > ?")
      .bind(now, body.request_id, caller.deviceId, now).run();
    if ((accepted.meta.changes || 0) !== 1) return reply({ error: "request_unavailable" }, 403, cors);
    await db.prepare("INSERT INTO device_links (link_id, device_a, device_b, created_at) VALUES (?, ?, ?, ?) ON CONFLICT(device_a, device_b) DO UPDATE SET link_id = excluded.link_id, created_at = excluded.created_at, revoked_at = NULL")
      .bind(linkId, a, b, now).run();
    return reply({ link_id: linkId, device_a: a, device_b: b }, 200, cors);
  }
  if (path === "/v1/links/reject" && method === "POST") {
    const body = await bodyOf(request);
    if (typeof body?.request_id !== "string") return reply({ error: "invalid_body" }, 400, cors);
    const result = await db.prepare("UPDATE device_link_requests SET rejected_at = ? WHERE request_id = ? AND target_device_id = ? AND accepted_at IS NULL AND rejected_at IS NULL AND expires_at > ?")
      .bind(now, body.request_id, caller.deviceId, now).run();
    return reply({ rejected: (result.meta.changes || 0) === 1 }, 200, cors);
  }
  if (path === "/v1/links/list" && method === "GET") {
    const rows = await db.prepare("SELECT l.link_id, l.device_a, l.device_b, l.created_at, d.device_name FROM device_links l JOIN devices d ON d.device_id = CASE WHEN l.device_a = ? THEN l.device_b ELSE l.device_a END WHERE (l.device_a = ? OR l.device_b = ?) AND l.revoked_at IS NULL ORDER BY l.created_at DESC")
      .bind(caller.deviceId, caller.deviceId, caller.deviceId).all();
    return reply({ links: rows.results || [] }, 200, cors);
  }
  if (path === "/v1/links/revoke" && method === "POST") {
    const body = await bodyOf(request);
    if (typeof body?.link_id !== "string") return reply({ error: "invalid_body" }, 400, cors);
    const result = await db.prepare("UPDATE device_links SET revoked_at = ? WHERE link_id = ? AND (device_a = ? OR device_b = ?) AND revoked_at IS NULL")
      .bind(now, body.link_id, caller.deviceId, caller.deviceId).run();
    if ((result.meta.changes || 0) === 1 && env.LINK_SIGNAL) {
      try {
        const room = env.LINK_SIGNAL.get(env.LINK_SIGNAL.idFromName(body.link_id));
        await room.fetch("https://link-signal.internal/revoke", {
          method: "POST", headers: { "X-Nexus-Revoke": "1" },
        });
      } catch { /* D1 revocation remains authoritative for reconnects. */ }
    }
    return reply({ revoked: (result.meta.changes || 0) === 1 }, 200, cors);
  }
  return reply({ error: "not_found" }, 404, cors);
}
