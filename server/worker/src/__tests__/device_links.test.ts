import { describe, expect, test } from "vitest";
import { authenticateLinkedDevice } from "../route_auth";
import { sha256Hex } from "../identity";
import { orderedDevices, validPhone } from "../device_links";
import { memoryIdentityStore } from "./identity_helpers";
import worker from "../index";
import { miniD1 } from "./mini_d1";

describe("Device Links identity boundary", () => {
  test("an installed pending device works without an AI entitlement", async () => {
    const store = memoryIdentityStore();
    const token = "device-secret";
    store.profiles.set("prof_pending", {
      profile_id: "prof_pending", human_label: null, status: "pending",
      quota_tier: null, created_at: 1, approved_at: null, approved_by: null,
      expires_at: null, last_seen_at: null, provision_hint: null,
    });
    store.devices.set("dev_a", {
      device_id: "dev_a", profile_id: "prof_pending", device_token_hash: await sha256Hex(token),
      device_name: "Laptop", os: "windows", app_version: "1", status: "active",
      created_at: 1, last_seen_at: null, revoked_at: null,
    });
    const request = new Request("https://nexus.test/v1/links/list", { headers: {
      Authorization: `Bearer ${token}`, "X-Nexus-Profile-Id": "prof_pending",
      "X-Nexus-Device-Id": "dev_a",
    } });
    expect(await authenticateLinkedDevice(store, request)).toEqual({
      ok: true, profileId: "prof_pending", deviceId: "dev_a",
    });
    store.devices.get("dev_a")!.status = "revoked";
    expect(await authenticateLinkedDevice(store, request)).toMatchObject({ ok: false, code: "revoked" });
    store.devices.get("dev_a")!.status = "active";
    store.profiles.get("prof_pending")!.status = "suspended";
    expect(await authenticateLinkedDevice(store, request)).toMatchObject({ ok: false, code: "suspended" });
    store.profiles.get("prof_pending")!.status = "revoked";
    expect(await authenticateLinkedDevice(store, request)).toMatchObject({ ok: false, code: "revoked" });
  });

  test("wrong token and another profile cannot claim a device", async () => {
    const store = memoryIdentityStore();
    store.profiles.set("prof_a", {
      profile_id: "prof_a", human_label: null, status: "approved",
      quota_tier: null, created_at: 1, approved_at: 1, approved_by: "test",
      expires_at: null, last_seen_at: null, provision_hint: null,
    });
    store.profiles.set("prof_b", {
      profile_id: "prof_b", human_label: null, status: "pending",
      quota_tier: null, created_at: 1, approved_at: null, approved_by: null,
      expires_at: null, last_seen_at: null, provision_hint: null,
    });
    store.devices.set("dev_a", {
      device_id: "dev_a", profile_id: "prof_a", device_token_hash: await sha256Hex("real"),
      device_name: null, os: null, app_version: null, status: "active",
      created_at: 1, last_seen_at: null, revoked_at: null,
    });
    const make = (profile: string, token: string) => new Request("https://nexus.test/v1/links/list", {
      headers: { Authorization: `Bearer ${token}`, "X-Nexus-Profile-Id": profile,
        "X-Nexus-Device-Id": "dev_a" },
    });
    expect(await authenticateLinkedDevice(store, make("prof_a", "wrong")))
      .toMatchObject({ ok: false, code: "bad_token" });
    expect(await authenticateLinkedDevice(store, make("prof_b", "real")))
      .toMatchObject({ ok: false, code: "unknown_device" });
  });
});

describe("Device Links inputs", () => {
  test("only normalized international numbers are accepted", () => {
    expect(validPhone("+919876543210")).toBe(true);
    for (const value of ["9876543210", "+01234", "+1 212 555 1234", "++12125551234", "abc", null]) {
      expect(validPhone(value)).toBe(false);
    }
  });
  test("a pair has one stable canonical ordering", () => {
    expect(orderedDevices("dev_z", "dev_a")).toEqual(["dev_a", "dev_z"]);
    expect(orderedDevices("dev_a", "dev_z")).toEqual(["dev_a", "dev_z"]);
  });
});

test("pending profiles reach links while the absent SMS gateway fails closed", async () => {
  const env: any = {
    DB: miniD1(), AI: { run: async () => ({}) }, MIGRATION_MODE: "strict",
  };
  const claim = await (await worker.fetch(new Request("https://nexus.test/v1/profiles/claim", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ provisional_user_id: "link-test", device_name: "Test", os: "windows" }),
  }), env)).json() as any;
  const headers = {
    Authorization: `Bearer ${claim.device_token}`,
    "X-Nexus-Profile-Id": claim.profile_id,
    "X-Nexus-Device-Id": claim.device_id,
    "Content-Type": "application/json",
  };
  const request = new Request("https://nexus.test/v1/links/phone/start", {
    method: "POST", headers, body: JSON.stringify({ phone: "+919876543210" }),
  });
  const unavailable = await worker.fetch(request, env);
  expect(unavailable.status).toBe(503);
  expect((await unavailable.json() as any).error).toBe("sms_gateway_unavailable");
  const unauth = await worker.fetch(new Request("https://nexus.test/v1/links/phone/start", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ phone: "+919876543210" }),
  }), env);
  expect(unauth.status).toBe(401);
});
