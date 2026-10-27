const SOCKET_URL = "ws://127.0.0.1:39221";
let socket;
let ticker;
let permissionGranted = false;

async function sendFocusedDomain() {
  if (!permissionGranted || !socket || socket.readyState !== WebSocket.OPEN) return;
  const { token, browser } = await chrome.storage.local.get(["token", "browser"]);
  if (!/^[a-f0-9]{64}$/i.test(token ?? "") || !["chrome", "brave", "edge"].includes(browser)) return;
  let domain = null;
  try {
    // Private windows are not visible to this extension. A normal window must
    // affirmatively be focused; a cached tab is never counted as active.
    const windows = await chrome.windows.getAll({ populate: true });
    const focused = windows.find((window) => window.focused && !window.incognito);
    const tab = focused?.tabs?.find((candidate) => candidate.active);
    if (tab?.url) {
      const url = new URL(tab.url);
      if (url.protocol === "http:" || url.protocol === "https:") domain = url.hostname;
    }
  } catch { /* Unknown browser state is no site activity. */ }
  socket.send(JSON.stringify({ token, browser, domain }));
}

function connect() {
  if (socket && [WebSocket.CONNECTING, WebSocket.OPEN].includes(socket.readyState)) return;
  chrome.storage.local.get(["token", "browser"]).then(({ token, browser }) => {
    if (!/^[a-f0-9]{64}$/i.test(token ?? "") || !["chrome", "brave", "edge"].includes(browser)) return;
    socket = new WebSocket(SOCKET_URL);
    socket.onopen = () => {
      socket.send(JSON.stringify({ token, browser, domain: null }));
    };
    socket.onmessage = (event) => {
      let response;
      try { response = JSON.parse(event.data); } catch { socket?.close(); return; }
      if (response.enabled !== true) { socket?.close(); return; }
      if (permissionGranted) return;
      permissionGranted = true;
      void sendFocusedDomain();
      clearInterval(ticker);
      ticker = setInterval(() => void sendFocusedDomain(), 2000);
    };
    socket.onclose = () => { permissionGranted = false; clearInterval(ticker); socket = undefined; };
    socket.onerror = () => { socket?.close(); };
  });
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.alarms.create("nexus-reconnect", { periodInMinutes: 0.5 });
  connect();
});
chrome.runtime.onStartup.addListener(connect);
chrome.alarms.onAlarm.addListener((alarm) => { if (alarm.name === "nexus-reconnect") connect(); });
chrome.windows.onFocusChanged.addListener(() => void sendFocusedDomain());
chrome.windows.onRemoved.addListener(() => void sendFocusedDomain());
chrome.tabs.onActivated.addListener(() => void sendFocusedDomain());
chrome.tabs.onUpdated.addListener((_id, change) => {
  if (change.url || change.status === "complete") void sendFocusedDomain();
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local" || (!changes.token && !changes.browser)) return;
  socket?.close();
  connect();
});
connect();
