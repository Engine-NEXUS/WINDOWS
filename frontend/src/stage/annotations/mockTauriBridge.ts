// Minimal mock of the Tauri JS bridge on window for dev-only harness
let callbackId = 0;
const handlers: Record<string, number[]> = {};

/* eslint-disable @typescript-eslint/no-explicit-any */
(window as any).__TAURI_INTERNALS__ = {
  transformCallback(cb: (data: unknown) => void) {
    const id = ++callbackId;
    (window as any)[`_${id}`] = cb;
    return id;
  },
  invoke(cmd: string, args?: Record<string, unknown>) {
    if (cmd === "plugin:event|listen") {
      const event = args?.event as string;
      const handler = args?.handler as number;
      if (!handlers[event]) handlers[event] = [];
      handlers[event].push(handler);
      return Promise.resolve(handler);
    }
    if (cmd === "plugin:event|unlisten") {
      const event = args?.event as string;
      const eventId = args?.eventId as number;
      if (handlers[event]) {
        handlers[event] = handlers[event].filter((h) => h !== eventId);
      }
      return Promise.resolve();
    }
    return Promise.resolve(null);
  },
  convertFileSrc: (s: string) => s,
};

window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: () => {},
};

export function emit(event: string, payload: unknown): void {
  const list = handlers[event] || [];
  for (const handlerId of list) {
    const fn = (window as any)[`_${handlerId}`];
    if (typeof fn === "function") {
      fn({ event, id: 1, payload });
    }
  }
}
