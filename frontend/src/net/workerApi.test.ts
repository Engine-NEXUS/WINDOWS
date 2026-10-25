import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

import { WorkerAuthError, workerJson, workerRequest } from "./workerApi";

beforeEach(() => invokeMock.mockReset());

describe("workerRequest", () => {
  it("goes through the worker_request command and never carries credentials itself", async () => {
    invokeMock.mockResolvedValue({ status: 200, body: { ok: true }, message: null });
    await workerRequest("GET", "/oauth/status");
    expect(invokeMock).toHaveBeenCalledWith("worker_request", {
      method: "GET",
      path: "/oauth/status",
      body: null,
    });
    const args = JSON.stringify(invokeMock.mock.calls[0]);
    expect(args).not.toMatch(/authorization|bearer|device_token/i);
  });

  it("passes a body through", async () => {
    invokeMock.mockResolvedValue({ status: 200, body: {}, message: null });
    await workerRequest("DELETE", "/oauth/disconnect", { provider: "google" });
    expect(invokeMock.mock.calls[0][1].body).toEqual({ provider: "google" });
  });
});

describe("workerJson", () => {
  it("returns the body of a 2xx", async () => {
    invokeMock.mockResolvedValue({ status: 200, body: { url: "https://x" }, message: null });
    await expect(workerJson("GET", "/oauth/auth-url?provider=google")).resolves.toEqual({ url: "https://x" });
  });

  it("surfaces the plain-language reason for a denial as WorkerAuthError", async () => {
    invokeMock.mockResolvedValue({
      status: 401,
      body: { error: "auth_required" },
      message: "This device isn't linked to the NEXUS cloud service yet.",
    });
    const err = (await workerJson("GET", "/oauth/status").catch((e: unknown) => e)) as WorkerAuthError;
    expect(err).toBeInstanceOf(WorkerAuthError);
    expect(err.status).toBe(401);
    expect(err.message).toContain("isn't linked");
  });

  it("uses the server's error text for other failures", async () => {
    invokeMock.mockResolvedValue({ status: 400, body: { error: "unsupported provider: evil" }, message: null });
    await expect(workerJson("GET", "/oauth/auth-url?provider=evil")).rejects.toThrow("unsupported provider: evil");
  });

  it("falls back to a generic message when the body has no error", async () => {
    invokeMock.mockResolvedValue({ status: 502, body: "Bad gateway", message: null });
    await expect(workerJson("POST", "/oauth/exchange", {})).rejects.toThrow("(502)");
  });
});
