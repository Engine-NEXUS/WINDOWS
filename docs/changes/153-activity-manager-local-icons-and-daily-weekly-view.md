# Activity Manager: local icons and daily/weekly app view (2026-10-21)

## Built

- A dedicated Activity Manager page in the Command Hub, linked from Home and Device Links. Its 400 px layout has app-logo tiles, percentage chips, a time ring, a per-app daily history, and Today / 7 days / 30 days views.
- Eighty-six SVG app marks/icons are bundled under `frontend/public/activity-logos/`: 27 from pinned Iconify packages, 58 retrieved through the pinned theSVG MCP (including VLC), and one local generic-app fallback. Runtime icon display makes no request to Logo.dev or any other icon service. The page resolves process/display-name aliases to local files and uses the generic icon for an unrecognized app. Source versions, per-icon licenses, and refresh steps are in the asset directory README and `scripts/activity-icon-sources.json`.
- The 2026-10-21 Windows Start menu inventory had 218 entries, including desktop apps, operating-system tools, and Brave site shortcuts. The expanded catalog covers recognized installed brands and common site marks. It does not treat every Start entry as a separately trackable app. Activity Manager now gives known process stems readable names (for example, `Resolve.exe` becomes DaVinci Resolve and `studio64.exe` becomes Android Studio) and skips generic `ApplicationFrameHost.exe` and `msedgewebview2.exe` entries whose process names alone cannot identify the visible app.
- The existing encrypted, opt-in Windows foreground-app recorder now samples every two seconds and saves second-resolution time. Existing minute-based files remain readable. The sampler skips idle, private app names, and long suspend/scheduling gaps; 30-day expiry and local clear apply to both old and new data.
- Browser development mocks include sample app time so the narrow layout and navigation can be inspected without enabling real recording.

## Verified

- Frontend TypeScript and production build passed.
- All 86 SVGs parsed as XML and were checked for scripts, external references, and inline load handlers; all 57 curated MCP manifest entries had corresponding files.
- Codex's global `thesvg` MCP connection was configured at `@thesvg/mcp-server@0.8.4`; a live stdio handshake returned `search_icons`, `get_icon`, `list_variants`, `get_icon_url`, and `list_categories`. A search resolved `vlc-media-player`, and `get_icon` supplied the bundled VLC SVG. The importer retrieved the 57 additional curated SVGs without Logo.dev.
- The 400 px browser preview displayed Discord, Spotify, and Instagram logos; Today and 7-day totals and the VS Code daily-history drill-down were exercised.
- `cargo test --lib device_activity::tests --features custom-protocol,admin-brain`: 14 passed, one ignored live foreground probe, including the new installed-app label and generic-host checks. Other compilation warnings came from unrelated files.
- The ignored live foreground probe was then run explicitly on this Windows laptop. It observed `claude.exe`, idle time `Some(68)`, and the label `Claude`; the probe passed. This samples one foreground moment, not a full activity-recording session.
- The full frontend suite reached 634 passing tests and two failures in the concurrently edited `sidebar/stackView.test.tsx` (search expectation and a hook invoked outside React render). These tests do not exercise Activity Manager. A later Rust `cargo check` waited on other active Cargo jobs and was cancelled; the focused Activity Manager Rust test binary compiled and passed.

## Limits before a production claim

- No rebuilt Windows executable has been run through real foreground switching, sleep/wake, midnight rollover, idle transitions, or data-clear races. The UI preview used development sample data; the separate one-shot foreground probe passed.
- Website tracking was added in the subsequent [browser-site opt-in change](154-activity-manager-browser-site-opt-in.md); this record describes the app-only version built on 2026-10-21.
- Usage limits in the reference image are a design cue only; no enforcement or limit percentages are implemented.
- A two-second foreground sample estimates time and can assign up to one sample interval to the wrong app around rapid switches. This should be measured on real hardware before claiming exact usage accounting.
