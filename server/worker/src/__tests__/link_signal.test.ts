import { describe, expect, test } from "vitest";
import { LinkSignalRoom } from "../link_signal";

function room() {
  const saved = new Map<string, unknown>();
  const received: string[] = [];
  const closed: string[] = [];
  const sender = {
    deserializeAttachment: () => ({ deviceId: "dev_a" }),
    close: (_code: number, reason: string) => closed.push(reason),
  } as unknown as WebSocket;
  const peer = {
    deserializeAttachment: () => ({ deviceId: "dev_b" }),
    send: (message: string) => received.push(message),
  } as unknown as WebSocket;
  const storage = {
    get: async (key: string) => saved.get(key),
    put: async (key: string, value: unknown) => { saved.set(key, value); },
    transaction: async (fn: (txn: unknown) => Promise<unknown>) => fn(storage),
  };
  const state = {
    getWebSockets: () => [sender, peer],
    storage,
  } as unknown as DurableObjectState;
  return { signal: new LinkSignalRoom(state), sender, saved, received, closed };
}

describe("Device Links signaling boundary", () => {
  test("activity and file frames cannot traverse the Worker", async () => {
    const r = room();
    await r.signal.webSocketMessage(r.sender, JSON.stringify({ type: "activity", app: "Code", active_ms: 1000 }));
    await r.signal.webSocketMessage(r.sender, new ArrayBuffer(8));
    expect(r.received).toEqual([]);
    expect(r.closed).toContain("unsupported signaling frame");
    expect(r.closed).toContain("signaling frame too large");
  });

  test("message budget persists across a reconnect with the same device identity", async () => {
    const r = room();
    const presence = JSON.stringify({ type: "presence", active: true });
    for (let i = 0; i < 60; i++) await r.signal.webSocketMessage(r.sender, presence);
    expect(r.received).toHaveLength(60);
    const reconnect = {
      deserializeAttachment: () => ({ deviceId: "dev_a" }),
      close: (_code: number, reason: string) => r.closed.push(reason),
    } as unknown as WebSocket;
    await r.signal.webSocketMessage(reconnect, presence);
    expect(r.received).toHaveLength(60);
    expect(r.closed).toContain("signaling rate limit");
    expect(r.saved.has("quota:message:dev_a")).toBe(true);
  });
});
