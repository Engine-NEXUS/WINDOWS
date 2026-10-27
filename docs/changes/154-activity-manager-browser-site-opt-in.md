# Activity Manager browser-site opt-in (2026-10-22)

## Scope

- A second opt-in records website hostnames separately from app time. It requires local app recording to be enabled. Turning app recording off also turns website recording off; clear deletes both histories.
- A small Chromium extension is bundled in `src-tauri/resources/activity-manager-extension/` for Chrome, Brave, and Edge. The user loads it as an unpacked extension, chooses the matching browser, and enters a connection code shown by Nexus. Chrome's extension manifest sets `incognito` to `not_allowed`.
- The extension checks for a focused normal browser window and active HTTP(S) tab every two seconds and on tab/window focus changes. It sends only its hostname, browser choice, and connection code to `ws://127.0.0.1:39221`. No page title, path, query, full URL, browsing history, or remote icon request is sent. When no eligible focused tab exists it sends an empty domain.
- The first bridge message contains no site. Nexus replies whether both opt-ins are active; the extension sends site data only after an affirmative reply. If recording is turned off while connected, the next message is rejected and the connection closes. Nexus never stores it after opt-out.
- Nexus validates the connection code, browser name, and public DNS-shaped hostname. The current signal expires after 3.5 seconds. Website time is counted only if Windows also reports that same browser as the foreground process, the user is recently active, both consents are on, and the encrypted store is healthy. Stored domains and per-day seconds are pruned after 30 days. Website time is a subset of the browser app time; the UI labels it accordingly.
- Local app-time samples now split around midnight and discard large scheduler/suspend and wall-clock jumps. The Activity Manager lets the user clear unreadable encrypted history and retry a failed status fetch.
- Four more local brand SVGs (Google, Notion, OpenAI, Reddit) were retrieved through theSVG MCP for common website entries. Their per-icon licenses were checked against the theSVG registry and pinned in `scripts/activity-icon-sources.json`; the bundled catalog now has 90 files.

## Browser basis

- Chrome's [tabs API](https://developer.chrome.com/docs/extensions/reference/api/tabs) supplies the active tab URL to an extension with `tabs` permission. The extension reduces it to a hostname before sending it.
- Chrome's [incognito manifest key](https://developer.chrome.com/docs/extensions/reference/manifest/incognito) supports `not_allowed`; [windows API](https://developer.chrome.com/docs/extensions/reference/api/windows) exposes focused windows. The extension requires an affirmatively focused normal window, so a cached normal tab is not treated as foreground when an unseen private window takes focus.
- Chrome's [service worker lifecycle](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle) documents WebSocket traffic keeping a Manifest V3 worker active in Chrome 116 and later. A 30-second alarm reconnects if the local app starts later. Earlier browser versions or suspended workers may undercount; stale signals expire rather than being credited.

## Verification

Pending final native/browser walkthrough. Focused Rust tests cover token and domain validation, signal expiry, separate consents, midnight splitting, retention, sleep gaps, and encrypted store behavior. `scripts/activity-extension.test.mjs` checks that the extension sends no site before consent, never sends a full URL, and clears a normal-tab signal when a private window is focused. Frontend production build and extension JavaScript syntax check are required after final edits.

## Limits

- This is estimated foreground time from two-second samples; rapid tab or window switches can move one interval to the adjacent site or browser. It is not exact device usage telemetry.
- A private-window focus transition should clear the normal-tab signal immediately; if the browser delays or omits that event, its previous signal can persist for at most 3.5 seconds. This edge case needs a real browser walkthrough before claiming private-window exclusion is flawless.
- The extension must be installed separately in each supported browser; Firefox and browser PWAs with a different foreground executable are not covered. Private windows, IP-address sites, local names, internal pages, and tabs for which the browser withholds a URL are not recorded.
- A browser connection code is a local credential. Anyone who can read it can submit plausible hostnames to the loopback bridge; Nexus does not use it to run commands or share data. The code is distinct from the command webhook and agent-hook credentials.
- No website blocking or usage-limit enforcement exists; the reference design's percentages remain display-only.
