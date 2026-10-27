import { describe, expect, it } from "vitest";
import { emit } from "./mockTauriBridge";

describe("mockTauriBridge", () => {
  it("registers handler via invoke listen and emits payloads", async () => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown>; transformCallback: (cb: (data: unknown) => void) => number } }).__TAURI_INTERNALS__;
    expect(internals).toBeDefined();

    let received: unknown = null;
    const cb = (data: unknown) => {
      received = data;
    };
    const handlerId = internals.transformCallback(cb);

    const eventId = await internals.invoke("plugin:event|listen", {
      event: "test:event",
      handler: handlerId,
    });
    expect(eventId).toBe(handlerId);

    emit("test:event", { hello: "world" });
    expect(received).toEqual({
      event: "test:event",
      id: 1,
      payload: { hello: "world" },
    });
  });

  it("unregisters listener via invoke unlisten", async () => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown>; transformCallback: (cb: (data: unknown) => void) => number } }).__TAURI_INTERNALS__;

    let count = 0;
    const handlerId = internals.transformCallback(() => {
      count++;
    });

    await internals.invoke("plugin:event|listen", {
      event: "test:unlisten_event",
      handler: handlerId,
    });

    emit("test:unlisten_event", {});
    expect(count).toBe(1);

    await internals.invoke("plugin:event|unlisten", {
      event: "test:unlisten_event",
      eventId: handlerId,
    });

    emit("test:unlisten_event", {});
    expect(count).toBe(1);
  });

  it("resolves other commands to null", async () => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
    const res = await internals.invoke("stage_set_hitboxes", { source: "tour", rects: [] });
    expect(res).toBeNull();
  });
});
