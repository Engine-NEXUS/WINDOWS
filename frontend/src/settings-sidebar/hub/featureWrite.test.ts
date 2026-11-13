import { describe, expect, it, vi } from "vitest";
import { notTaken, writeAndVerify, type Invoke } from "./featureWrite";

/** A fake backend: `settings_patch` merges into `store` unless told to refuse or to ignore the write. */
function backend(opts: { refuse?: string; ignore?: boolean; readFails?: boolean } = {}) {
  const store: Record<string, unknown> = { screenTour: true, memcoreMail: true, watchPower: "off" };
  const calls: string[] = [];
  const invoke = (async (cmd: string, args?: Record<string, unknown>) => {
    calls.push(cmd);
    if (cmd === "settings_patch") {
      if (opts.refuse) throw new Error(opts.refuse);
      if (!opts.ignore) Object.assign(store, args?.patch as object);
      return undefined;
    }
    if (cmd === "get_settings") {
      if (opts.readFails) throw new Error("read failed");
      return { ...store };
    }
    return undefined;
  }) as Invoke;
  return { invoke, store, calls };
}

describe("writeAndVerify", () => {
  it("writes only the given key and confirms it by reading back", async () => {
    const b = backend();
    const r = await writeAndVerify(b.invoke, { screenTour: false });
    expect(r.ok).toBe(true);
    expect(r.settings?.screenTour).toBe(false);
    expect(b.store.memcoreMail).toBe(true); // untouched
    expect(b.calls).toEqual(["settings_patch", "get_settings"]);
  });

  it("a refused write is an error, nothing changes, and the real state is returned", async () => {
    const b = backend({ refuse: "no voice profile" });
    const r = await writeAndVerify(b.invoke, { screenTour: false });
    expect(r.ok).toBe(false);
    expect(r.error).toContain("no voice profile");
    expect(r.settings?.screenTour).toBe(true);
  });

  it("a write the backend silently ignored is reported, not shown as done", async () => {
    const b = backend({ ignore: true });
    const r = await writeAndVerify(b.invoke, { screenTour: false });
    expect(r.ok).toBe(false);
    expect(r.error).toContain("Narrated screen tour");
  });

  it("an enum that did not take is reported too", async () => {
    const b = backend({ ignore: true });
    const r = await writeAndVerify(b.invoke, { watchPower: "dry" });
    expect(r.ok).toBe(false);
    expect(r.error).toContain("Shut down when an agent finishes");
  });

  it("if the read-back fails the page does not claim success", async () => {
    const b = backend({ readFails: true });
    const r = await writeAndVerify(b.invoke, { screenTour: false });
    expect(r.ok).toBe(false);
    expect(r.settings).toBeNull();
    expect(r.error).toContain("could not read the settings back");
  });

  it("autostart also calls its own command first", async () => {
    const b = backend();
    const spy = vi.fn(b.invoke);
    await writeAndVerify(spy as Invoke, { autostart: false });
    expect(spy.mock.calls[0]).toEqual(["set_autostart", { enabled: false }]);
  });

  it("notTaken lists only the mismatches", () => {
    expect(notTaken({ screenTour: false, memcoreMail: true }, { screenTour: true, memcoreMail: true })).toEqual(["Narrated screen tour"]);
  });
});
