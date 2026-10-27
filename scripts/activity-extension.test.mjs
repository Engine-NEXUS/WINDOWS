import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../src-tauri/resources/activity-manager-extension/background.js", import.meta.url), "utf8");
const token = "a".repeat(64);
let windows = [{ focused: true, incognito: false, tabs: [{ active: true, url: "https://example.com/private/path?q=secret" }] }];
let ticker;
let socket;
let permit = true;
class FakeSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  constructor() {
    this.readyState = FakeSocket.OPEN;
    this.sent = [];
    socket = this;
    queueMicrotask(() => this.onopen());
  }
  send(payload) {
    this.sent.push(payload);
    if (this.sent.length === 1) queueMicrotask(() => this.onmessage({ data: JSON.stringify({ enabled: permit }) }));
  }
  close() { this.readyState = 3; this.onclose?.(); }
}
const event = () => ({ addListener() {} });
const chrome = {
  storage: { local: { get: async () => ({ token, browser: "chrome" }) }, onChanged: event() },
  windows: { getAll: async () => windows, onFocusChanged: event(), onRemoved: event() },
  tabs: { onActivated: event(), onUpdated: event() },
  runtime: { onInstalled: event(), onStartup: event() },
  alarms: { create() {}, onAlarm: event() },
};
vm.runInNewContext(source, { chrome, WebSocket: FakeSocket, URL, queueMicrotask,
  setInterval(callback) { ticker = callback; return 1; }, clearInterval() {} });
for (let i = 0; i < 3; i++) await new Promise(setImmediate);
assert.ok(socket?.sent.length, "extension sent a foreground heartbeat");
assert.equal(JSON.parse(socket.sent[0]).domain, null, "initial handshake sends no website");
let payload = JSON.parse(socket.sent.at(-1));
assert.equal(payload.domain, "example.com");
assert.ok(!socket.sent.at(-1).includes("secret") && !socket.sent.at(-1).includes("/private/"));

windows = [
  { focused: false, incognito: false, tabs: [{ active: true, url: "https://example.com/old" }] },
  { focused: true, incognito: true, tabs: [{ active: true, url: "https://private.example" }] },
];
await ticker();
for (let i = 0; i < 2; i++) await new Promise(setImmediate);
payload = JSON.parse(socket.sent.at(-1));
assert.equal(payload.domain, null, "a private foreground window never reuses the normal tab");

permit = false;
vm.runInNewContext(source, { chrome, WebSocket: FakeSocket, URL, queueMicrotask,
  setInterval(callback) { ticker = callback; return 2; }, clearInterval() {} });
for (let i = 0; i < 3; i++) await new Promise(setImmediate);
assert.equal(socket.sent.length, 1, "without Nexus consent only the empty handshake is sent");
assert.equal(JSON.parse(socket.sent[0]).domain, null);
console.log("Activity extension foreground/private-window checks passed.");
