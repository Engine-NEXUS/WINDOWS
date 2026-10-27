# REA Website Observation & THE LAW Compliance

**Date:** 2026-10-19. Status: RESEARCH (nothing built). Companion docs: [01 tool verification & install](01-rea-tool-verification-install-research-2026-10-19.md) · [03 teardown bay & diag design](03-teardown-bay-diag-design-research-2026-10-19.md) · [04 build plan](04-build-plan-research-2026-10-19.md).

**Source of truth for this doc:** REA's `docs/browser-observation.md` (main branch, fetched 2026-10-19). All mechanisms below are cited against that document unless noted.

---

## 1. Where websites sit in REA's capability map

From the repo README "What you can analyze" table, the rows relevant to NEXUS's WEBSITES-ONLY use:

| Target | What REA returns | Requirements (per README) |
|---|---|---|
| **Websites** | Page structure, scripts, network observations and requested screenshots | **A Chrome-family browser**; guide: `docs/browser-observation.md` |
| Saved network captures | Requests, responses, exposed payloads, source locations | HAR 1.2 or Fiddler SAZ; (Linux) mitmdump — optional future surface, not needed v1 |
| JavaScript / Electron | Modules, imports, source maps, routes, IPC, native add-on relationships | Node.js + npm; (Linux) procps-compatible `ps` — used for `export_web_scripts` output feeding static analysis |
| Static JavaScript + .NET | Files read **without running** the application | "Runtime capture runs or interacts…; each runtime guide describes its effects" — the JS-static lane is hands-off |

Everything else (Hopper/Ghidra/IDA binaries, APKs, firmware, ADB devices, packet captures, process capture) is out of NEXUS scope by the owner's WEBSITES-ONLY law and is never wired up.

## 2. The CDP attach model (how REA "sees" a website)

Quoted backbone: *"REA can attach to a user-owned Chrome-family browser through the CDP and produce bounded Evidence about an existing page. This is a passive reverse-engineering capability, **not a general browser automation or remote-control surface**."*

Facts the NEXUS teardown service must operate inside:

1. **REA never launches, owns, or terminates the browser.** The *caller* starts a dedicated Chromium-family browser with an explicit debugging port and a disposable profile:
   ```
   <chrome.exe> --remote-debugging-port=9222 --user-data-dir=<temp> [optional start url]
   ```
   NEXUS's teardown spawns Chrome itself (only when a session warrants it), with a temp profile dir, loopback flag only; the port value recorded in Evidence and torn down after captures.
2. **Valid endpoints are a strict literal set** — `http://127.0.0.1:PORT` or `[::1]`, or a browser socket `ws://127.0.0.1:PORT/devtools/browser/ID`. `localhost`, private-LAN addresses, HTTPS/wss endpoints, credentials, queries, fragments, extra paths and wildcards are **rejected as CDP inputs** — an inherited guard against the "attach to something else" mistake.
3. **Deadlines and message bounds.** Discovery 10 s deadline; each WebSocket handshake/command response 20 s (independent of the caller-chosen observation window — default 500 ms); 64 MiB per-WebSocket-message payload limit → `payload_limit` failure beyond that.
4. **Attach-only semantics.** Coverage starts only after REA attaches: `prior_activity_available` is always false — *absence from network/console arrays is not evidence an event never occurred*. Scripts are the exception (scripts parsed before attach are still listed). Honesty clause for Jarvis phrasing: REA evidence is *bounded*, and its completeness status fields (`complete_within_window`, `policy_filtered`, `attach_limited`, `truncated`) must reach the user claim.
5. **Origin boundary.** Follow-up inspection selects one target and its default scope is the page's *current origin* — plus explicit `allowed_origins` filters. Frames/scripts/workers/network events outside scope are excluded and counted, not exposed. If the main frame navigates during final capture, mixed results are discarded with `target_changed`.

## 3. Shipped surfaces (the passive set NEXUS may expose)

| CLI | MCP tool | What it does (per docs) |
|---|---|---|
| `rea list-browser-targets http://127.0.0.1:9222 --json` | `list_browser_targets` | Discovers eligible page targets at the endpoint, optional exact-origin filter (`--allowed-origins`) |
| `rea inspect-web-page <endpoint> <target> --observation-ms … --json` | `inspect_web_page` | DOM (node types/names/value lengths/attribute **names**, not values), accessibility (roles by default; names/descriptions only with `--include-accessibility-text`), scripts, resources, network (method/status/MIME/size/URL minus userinfo), console metadata, WebSocket frame *sizes*, workers, quota, optional storage **key names only** (values always redacted) |
| `rea analyze-web-bundle <endpoint> <target> --json` | `analyze_web_bundle` | Parses script artifacts **without execution**: chunk edges, route/endpoint candidates, vendor fingerprints, static WebMCP declarations, optional source-map evidence (fetch bounds: 30 s deadline, 64 MiB aggregate, 30 i per-map identity budgets) |
| `rea observe-web-session <endpoint> <target> --observation-ms … --json` | `observe_web_session` | Arms an observation window **for an external user action**: records ordered reload/SPA-navigation/redirect/failure/lifecycle and target-termination metadata. No input driven |
| `rea discover-webmcp-tools <endpoint> <target> --json` | `discover_webmcp_tools` | Read-only experimental CDP WebMCP domain: returns registrations + declared schemas; declarations untrusted JSON. **REA never exposes `WebMCP.invokeTool`** |
| `rea capture-web-screenshot <endpoint> <target> --json` | `capture_web_screenshot` | Content-addressed visible-viewport PNG (sole pixel surface) |
| `rea compare-web-captures` / `compare-web-screenshots` | `compare_web_captures` / `compare_web_screenshots` | Bounded local comparison (screenshot budget 256 MiB working-memory for PNG inflation); missing/truncated evidence never treated as equivalence |
| — (`export_web_scripts` per `docs/website-script-export.md`, referenced) | `export_web_scripts` | Exports captured Debugger sources to REA's static JS analysis/std (needs JS analysis surface, no native engine) |

Every surface has "equivalent CLI and MCP contracts and returns Evidence provenance" and returns "complete results inline with an Evidence ID".

## 4. Data-minimization posture (why the owner can read the evidence)

Defaults straight from `browser-observation.md` §Data minimization:

- URL observations: **only username/password userinfo removed**; query values/fragments/order retained
- DOM snapshots: node types/names/value lengths/attribute names, **not text or attribute values**
- Network: **cookies and authorization headers never retained**; headers discarded except an allowlisted projection (length/encoding, CSP/Link/COOP/COEP fields, untrusted agent hints); request/response **bodies not requested/parsed by default** (`Network.getResponseBody` never sent unless JSON-body-shape capture is explicitly selected; when selected, JSON converted immediately to *value-free typed paths*)
- Console: call types/argument types/timestamps/location; **primitive text only when explicitly requested** (may contain secrets — NEXUS leaves it off by default, or gates per-intent)
- WebSocket: direction/opcode/byte lengths; **shape capture = JSON paths/types/counts, never values or examples**; binary bytes never retained
- Storage: **values always redacted** even when key names are requested; fingerprints only with the additional selection
- Script `sources`: omitted unless explicitly requested; returned inline **with SHA-256 digest**
- Redirect chain hops outside selected origins discarded; `Network.getResponseBody` never sent without selection

## 5. THE LAW — clause mapping

Owner's law: *"REA observes. It never clicks, types, logs in, or changes anything, and Jarvis never points it at anything but websites I may look at."* Mapping to REA's attested mechanism + the NEXUS enforcement point:

| Law clause | REA guarantee (source: browser-observation.md §Non-goals) | NEXUS enforcement (teardown/orchestrator) |
|---|---|---|
| **Never clicks, types** | "deliberately does not expose generic CDP commands, `Runtime.evaluate`, WebMCP invocation, **navigation, input**, downloads, page closure, or browser closure" | The scenario/Playwright provider family (`capture-browser-scenario`, Electron scenarios) **is never callable** from NEXUS's bay: teardown whitelists only the §3 passive commands; bay code imports no scenario surface; Jarvis's intent router has no verb for scenario capture |
| **Never logs in** | No input/evaluate surface = no credential entry; auth happens *outside* REA (the human's normal browser profile, untouched) | NEXUS inspect targets start from a **disposable profile** (temp `--user-data-dir`, login-free); Evidence's stored URLs have userinfo stripped by REA anyway |
| **Never changes anything** | Disables the CDP domains it enabled, detaches the session, closes only its own WebSocket; "not a browser sandbox… the attached page may make external requests independently" (documented residual, not a REA violation) | Teardown records the residual honestly in evidence summaries (~"the page kept living on its own; REA only watched it") |
| **Never points it at anything but websites I may look at** | REA attaches to what it is *given*; no URL navigation exists at all | Two-layer: (a) the orchestrator only launches teardown sessions for targets passing the site-scope check (public/open-source/owner projects); (b) the browser itself starts from an allow-listed loopback port with a fresh profile — nothing from the owner's real browser state leaks in |
| Observe never changes the *page state* | REA can clear another client's `navigator.platform` emulation on disconnect (Chromium behavior, issue #4090 — documented limitation) | Recorded as a known side-effect; teardown notes it whenever another tool was driving the page |

**One real watch-item (the only documented exception surface):** REA *also* ships a Playwright-scenario provider ("drive actions and capture step snapshots in either a provider-owned browser or a caller-selected external browser" — §Provider authorities). This is the one family that contradicts observe-only. **Decision:** NEXUS simply never exposes it; the teardown surface list in doc 03 is the enforceable whitelist, and T4 tests assert scenario commands are unreachable (doc 04 §5).

## 6. The workflow Jarvis will actually run (research sketch)

```
T1  locate Chrome → spawn <chrome.exe> --remote-debugging-port=<E> --user-data-dir=<fresh temp dir>
T2  rea list-browser-targets http://127.0.0.1:<E> --allowed-origins <site-origin> --json
T3  pick target id → rea inspect-web-page <endpoint> <target> --observation-ms <w> --json
T4  optional: rea capture-web-screenshot (content-addressed PNG for the user's view)
T5  optional: rea analyze-web-bundle → chunk edges/routes/vendors (+ source-map fetch when asked)
T6  optional: rea export-web-scripts → feeds static JS analysis of the site's bundles
T7  rea compare-web-captures left.json right.json (before/after a code change, offline)
T8  teardown service kills the spawned Chrome instance + temp profile; REA session ended by detach
```

The scheme, evidence and claims keyed to the underpinning (`list-browser-targets` first, because target IDs are browser-instance-specific).

## 7. Completeness/limits & open questions (not verified)

1. **Completeness vocabulary matters for honest output**: `complete_within_window` vs `policy_filtered` vs `attach_limited` vs `truncated` — the teardown projection must surface these, or Jarvis overclaims.
2. `inspect_web_page` snapshot budget: script metadata admission bounds retained identity/source-map strings (8 MiB / 50,000 records); overflow flagged `resource_budget_exhausted` — parse them into the user-visible "any incomplete coverage reports" slot rather than swallowing.
3. **WebMCP discovery is browser-version-dependent** (Chrome 149+ trial/`--enable-features=WebMCPTesting`; one observed Chrome 154 answered by default). NEXUS does not depend on it in v1 (nice-to-have tool listing).
4. **Scenario provider residual risk**: re-confirm in T4 that the bay's subprocess environment provably cannot reach `capture-browser-scenario` even through a hand-typed `node rea.mjs …` invocation attempt by some other path (test asserts the bay's whitelist rejects it at the *route* layer).
5. macOS/Windows headless Chromium diffs for screenshots — unverified for NEXUS until T4 live runs (Windows Chrome present; macOS only researched from docs).
6. Browsers without HTTP discovery (`/json/version` absent; Chrome 144+ `chrome://inspect/#remote-debugging`, socket-only mode): supported via `ws://…/devtools/browser/<id>` for select commands — worked only if teardown ever meets such a target; v1 sticks to standard `--remote-debugging-port` spawns.

## 8. Sources (checked 2026-10-19)

- **`docs/browser-observation.md` (raw fetch, morluto/rea main)** — §2 quoted backbone, §3 surfaces table incl. exact flag names/defaults, §4 minimization list quotes, deadline/budget numbers, WebMCP prerequisites, socket-mode rules, issue #4089 emulation note, "Non-goals and threat model" list
- **`docs/installation.md` (raw fetch)** — release-boundary text ("Released package and main"), Windows Ghidra P0 scope (why NEXUS doesn't need it), Linux `ps` prerequisite for JS analysis, macOS Hopper segments (out of scope, cited for the "why excluded")
- **`docs/cli.md` (raw fetch)** — exit statuses for browser-adjacent commands, `analyze-web-bundle` signal handling, Evidence save (`> app-evidence.json`) + view projection pattern re-usable for compare captures
- **Repository README (main branch)** — "Websites" row + requirements, "Runtime capture runs or interacts" honesty line, `npx -y rea-agents@latest analyze-javascript-application` standalone usage, DISCLAIMER pointer
- **`docs/website-script-export.md` / `docs/website-module-trace.md`** — referenced by name from browser docs for the export surface; not yet fetched (deferred: only needed once T6 exists) — flagged honestly as *referenced, not verified*
- **REA `DISCLAIMER.md`** — referenced by the README for responsible use ("lawful research…software you own or are authorized to examine"); not fetched directly this session — flagged as *referenced, not verified*
