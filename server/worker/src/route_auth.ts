/**
 * Route authentication for the endpoints that hand out or mutate provider
 * credentials (/oauth/*, /apikeys/*, /models/*).
 *
 * Before this module those routes trusted a caller-supplied `user_id`, so anyone who knew
 * (or guessed) an id could read that user's GitHub/Google/Swiggy access tokens. Now a
 * caller must prove it is an approved device:
 *
 *   Authorization: Bearer <device token>
 *   X-Nexus-Profile-Id: <profile id>      (or ?profile_id=)
 *   X-Nexus-Device-Id:  <device id>       (or ?device_id=)
 *
 * The canonical path of `resolveProfile` is used regardless of MIGRATION_MODE, so the
 * legacy "id alone is enough" mode can never authorise one of these routes.
 *
 * The browser OAuth callback cannot carry a bearer header (the provider redirects the
 * user's browser to it), so it is bound to the authenticated request that started the flow
 * through a single-use server-side state nonce instead.
 */

import { resolveProfile, sha256Hex, timingSafeEqualHex, type IdentityStore } from "./identity";

export const PROFILE_HEADER = "X-Nexus-Profile-Id";
export const DEVICE_HEADER = "X-Nexus-Device-Id";

export type CallerAuth =
  | { ok: true; profileId: string; deviceId: string; ownerKey: string }
  | { ok: false; status: 401 | 403 | 503; code: string };

/**
 * Device Links authenticate an installed device, not its Workers AI grant.
 * A pending profile can link laptops, while a revoked device cannot. This
 * intentionally does not call resolveProfile, whose strict entitlement gate
 * is for cloud AI and credential routes.
 */
export async function authenticateLinkedDevice(
  store: IdentityStore,
  request: Request,
): Promise<{ ok: true; profileId: string; deviceId: string } | { ok: false; status: 401 | 403 | 503; code: string }> {
  const profileId = request.headers.get(PROFILE_HEADER)?.trim() || "";
  const deviceId = request.headers.get(DEVICE_HEADER)?.trim() || "";
  const bearer = request.headers.get("Authorization") || "";
  if (!profileId || !deviceId || !bearer.startsWith("Bearer ")) {
    return { ok: false, status: 401, code: "auth_required" };
  }
  const token = bearer.slice(7);
  if (!token || token.length > 512) return { ok: false, status: 401, code: "bad_token" };
  try {
    const profile = await store.getProfile(profileId);
    if (!profile) return { ok: false, status: 401, code: "unknown_profile" };
    if (profile.status === "revoked" || profile.status === "suspended") {
      return { ok: false, status: 403, code: profile.status };
    }
    const device = await store.getDeviceForProfile(deviceId, profileId);
    if (!device) return { ok: false, status: 401, code: "unknown_device" };
    if (device.status !== "active") return { ok: false, status: 403, code: "revoked" };
    if (!timingSafeEqualHex(await sha256Hex(token), device.device_token_hash)) {
      return { ok: false, status: 401, code: "bad_token" };
    }
    return { ok: true, profileId, deviceId };
  } catch {
    return { ok: false, status: 503, code: "lookup_failed" };
  }
}

/**
 * Authenticate the caller as an approved device and decide which key owns the data.
 *
 * `suppliedUserId` is whatever the client put in the request. It is accepted only when it
 * is the caller's own profile id or a legacy id that the identity migration mapped to this
 * profile (so tokens stored before the migration stay reachable); anything else is 403.
 */
export async function authenticateCaller(
  store: IdentityStore,
  request: Request,
  suppliedUserId: string | null | undefined,
  now: number,
): Promise<CallerAuth> {
  const url = new URL(request.url);
  const profileId = (request.headers.get(PROFILE_HEADER) || url.searchParams.get("profile_id") || "").trim();
  const deviceId = (request.headers.get(DEVICE_HEADER) || url.searchParams.get("device_id") || "").trim();
  const authHeader = request.headers.get("Authorization");

  if (!profileId || !deviceId || !authHeader) {
    return { ok: false, status: 401, code: "auth_required" };
  }

  const resolved = await resolveProfile(store, { profileId, deviceId, authHeader, mode: "strict" }, now);
  if (!resolved.ok) {
    const status = resolved.code === "bad_token" || resolved.code === "unknown_profile"
      ? 401
      : resolved.code === "lookup_failed" ? 503 : 403;
    return { ok: false, status, code: resolved.code };
  }

  const owner = await ownerKeyFor(store, resolved.profileId, suppliedUserId);
  if (!owner.ok) return owner;
  return { ok: true, profileId: resolved.profileId, deviceId: resolved.deviceId, ownerKey: owner.ownerKey };
}

/**
 * Decide which storage key a client-supplied `user_id` maps to for an authenticated
 * profile: the profile itself, or a legacy id the migration table links to it. Used
 * directly by routes that only learn the `user_id` after reading the request body.
 */
export async function ownerKeyFor(
  store: IdentityStore,
  profileId: string,
  suppliedUserId: string | null | undefined,
): Promise<{ ok: true; ownerKey: string } | { ok: false; status: 403 | 503; code: string }> {
  const supplied = (suppliedUserId || "").trim();
  if (!supplied || supplied === profileId) return { ok: true, ownerKey: profileId };
  let migrated: { profile_id: string } | null = null;
  try {
    migrated = await store.getMigration(supplied);
  } catch {
    return { ok: false, status: 503, code: "lookup_failed" };
  }
  if (!migrated || migrated.profile_id !== profileId) {
    return { ok: false, status: 403, code: "forbidden_user" };
  }
  return { ok: true, ownerKey: supplied };
}

// ---- OAuth state nonce (CSRF + account-binding) ----

const STATE_PREFIX = "oauth_state:";
const STATE_TTL_SECONDS = 600;

export interface OAuthStateRecord {
  provider: string;
  ownerKey: string;
}

function randomHex(bytes: number): string {
  const buf = new Uint8Array(bytes);
  crypto.getRandomValues(buf);
  return Array.from(buf, b => b.toString(16).padStart(2, "0")).join("");
}

/** Mint an unguessable single-use state bound to the authenticated owner + provider. */
export async function createOAuthState(
  kv: KVNamespace,
  provider: string,
  ownerKey: string,
): Promise<string> {
  const state = randomHex(24);
  const record: OAuthStateRecord = { provider, ownerKey };
  await kv.put(STATE_PREFIX + state, JSON.stringify(record), { expirationTtl: STATE_TTL_SECONDS });
  return state;
}

/** Look the state up and burn it. Returns null when unknown, expired or already used. */
export async function consumeOAuthState(
  kv: KVNamespace,
  state: string,
): Promise<OAuthStateRecord | null> {
  if (!/^[0-9a-f]{48}$/.test(state)) return null;
  const raw = await kv.get(STATE_PREFIX + state);
  if (!raw) return null;
  await kv.delete(STATE_PREFIX + state);
  try {
    const rec = JSON.parse(raw) as OAuthStateRecord;
    if (!rec.provider || !rec.ownerKey) return null;
    return rec;
  } catch {
    return null;
  }
}

// ---- Small shared helpers ----

/** Escape text for safe interpolation into HTML (element content and quoted attributes). */
export function escapeHtml(value: string): string {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Constant-time string comparison (length is not secret here, content is). */
export function timingSafeEqualStr(a: string, b: string): boolean {
  const enc = new TextEncoder();
  const x = enc.encode(a);
  const y = enc.encode(b);
  const len = Math.max(x.length, y.length);
  let diff = x.length ^ y.length;
  for (let i = 0; i < len; i++) diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return diff === 0;
}

// ---- CORS ----

/** Origins the NEXUS desktop WebView loads from (Windows, macOS/Linux, and local dev). */
const ALLOWED_ORIGINS = new Set([
  "http://tauri.localhost",
  "https://tauri.localhost",
  "tauri://localhost",
  "http://localhost:5173",
]);

/** CORS headers for this request's Origin; empty (no ACAO) when the origin is not allowed. */
export function corsHeaders(request: Request): Record<string, string> {
  const origin = request.headers.get("Origin") || "";
  const headers: Record<string, string> = { Vary: "Origin" };
  if (ALLOWED_ORIGINS.has(origin)) headers["Access-Control-Allow-Origin"] = origin;
  return headers;
}
