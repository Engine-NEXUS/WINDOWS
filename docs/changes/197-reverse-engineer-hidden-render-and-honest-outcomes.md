# 197 — Reverse engineer: hidden render, honest outcomes, mouse-free tour (2026-11-14)

Plan: `C:\Users\Chitkul Lakshya\.claude\plans\velvety-cooking-barto.md`. Owner choices: add a **hidden render** pass;
scope is **make it work well** (no security grade, no rebuild plan, no extension). Source edits and offline checks only:
nothing was built, launched or run live.

## Why

"Nexus, reverse engineer this website/app" (change 152) was wired but never run live. The 2026-11-01 audit
(`docs/research/capabilities-audit/03-reverse-engineering.md`) and a fresh trace of the code found 21 defects. The worst:

- A refused or failed scan still ran the tour and ended "Walkthrough complete… found 0 technologies".
- The tour moved the user's real cursor and sent real wheel events.
- Nothing could stop the slow steps.
- App mode described a desktop app as "this website".
- Inline `<script>` bodies were never searched.
- Single-page apps, sign-in walls and bot walls came back nearly empty with no explanation.
- Common phrasings were missed ("what's this site built with", "is this WordPress").

## What changed

### Honest outcomes (`reverse_engineer.rs`)

- New `ScanOutcome` (Refused / Failed / Thin / Ok) decides the tour and the last line:
  - Refused targets (private address, banking page, system app) are never captured or toured.
  - Failed scans give a visual-only tour and say so.
  - Thin results say why.
  - The closing line can no longer claim "complete" after a refusal or say "found 0 technologies".
- Walkthrough sections are kept on the failure path (`stack_payload_failed`).
- Cancel (barge-in or stop) now ends the probe wait, each vision call and the app scan within about 100 ms (`cancellable`).
  The app scan also has an 8 s cap.
- Loading: `LoadingEnd` is sent once, and only if the spinner was shown.
- Meeting mode no longer docks the sidebar on a screen share.
- NEXUS's own sidebar is cropped out of the screenshots (`capture_region_excluding`).
  Display-affinity exclusion is unreliable under GDI.
- App mode:
  - A scanning card appears before the scan.
  - It says when the window could not be analysed.
  - The payload carries `kind: "app"` and the exe **file name** only (the full path can carry the user name).
  - The report heading is "App walkthrough".
- Spoken wording follows confidence:
  - Detected: "This is built with X".
  - Likely: "looks like X".
  - Inferred: "my best guess is X".
  - The thin reason is appended.
- `screen_tour::build_reverse_prompt` takes `RevTarget::{Site, App}`. App prompts never mention pages, scrolling or footers.
- `intent_parser` normalises curly apostrophes, polite lead-ins and "what's", and adds:
  - "what's this built with"
  - "what this site is built with"
  - "what framework/cms/stack is/does this use"
  - "is this (site) wordpress/shopify/react/next.js/…"

  "app store" phrasings are refused.

### Static detection (`site_probe.rs`, `site_fingerprints.json`)

- **Inline scripts** are searched by `script_text`. Caps: 64 KB each, 512 KB total.
- **Same site** uses `registrable_domain` with a curated multi-label suffix list:
  - country suffixes such as `co.uk` and `com.au`
  - platform suffixes such as `vercel.app`, `github.io` and `pages.dev`

  Every `*.co.uk` host is no longer "the same site".
- **Implies**:
  - only from parents at 0.50 or more
  - two levels deep
  - an already-found child is raised, never lowered
  - implied children below 0.30 are no longer added
- **Why thin**: `page_health` returns `ThinReason` values.
  - `Challenge` covers Cloudflare (`cf-mitigated`, "Just a moment", challenge-platform only on 401/403/429/503), Imperva,
    DataDome, PerimeterX and Akamai. Normal Cloudflare pages that carry the challenge script are not flagged.
  - `LoginWall` covers a login path or a redirect through a Google, Microsoft, Apple, Okta, Auth0 or OneLogin sign-in page.
  - `SpaShell` is an empty root with under 200 characters of text.
  - `HttpError` covers other error statuses.

  The reasons are spoken, shown in the sidebar and written in the report.
- **User agent**: a plain Chrome UA plus `Accept-Language`. The `NEXUS-StackScan/1.0` bot token invited WAF challenges.
- **Version capture**: `Detection.version`, accepted only if it looks like a version.
- `fetch_and_analyse` is split into `fetch_raw` (network) and `analyse_captured` (pure), so replayed fixtures run the exact
  live analysis.
- **Table**: 267 → 244 entries.
  - The duplicate Microsoft Clarity entry was merged; the bare "clarity" 0.9 signal is now `clarity.ms`.
  - About 20 loose short substrings at weight 0.5 or more became word-bounded regexes ("lit", "aos", "fa-", "FS.", …).
  - Elementor and Phosphor Icons got real signals.
  - 22 build/test tools that cannot be seen from a live page were removed: rollup, typescript, eslint, prettier, jest,
    vitest, playwright, cypress, storybook, formik, swr, pinia, vuex, xstate, wouter, remark, rehype, unified, sharp,
    imagemin, heroicons and elasticsearch.
  - The fixture replay exposed a real bug: `cf-ray` alone (0.9) marked every Cloudflare-proxied site "Cloudflare Pages,
    Detected". It is now a supporting hint only; `*.pages.dev` stays strong.

### Hidden render (`site_render.rs`, new)

- After a successful static scan, the guarded, query-stripped `https` URL is loaded in an 8×8 window labelled `stack-render`.
  - It sits almost entirely off the top-left corner, because a hidden WebView2 is throttled.
  - It is InPrivate, click-through and excluded from capture.
  - Its label is in no capability file, so the page has no Tauri IPC (a test checks the capability files).
- WebView2 hardening:
  - every permission denied
  - audio muted
  - new windows denied
  - downloads cancelled
  - `WebResourceRequested` answers 403 for private/loopback IP literals, `.local`/`.internal`-style names and bare
    single-label hosts
  - CDP `Emulation.setDeviceMetricsOverride` lays the page out as a 1366×900 desktop
- Navigation is re-checked with the probe's own `guard_url` (https, public, not sensitive), with at most 6 hops. A redirect to
  a sign-in service is refused and reported as a login wall.
- Timing: the collector runs after the load event plus 1.5 s (at most 8 s). The window is always destroyed, with a 12 s hard
  cap and a cancel check at every step.
- The collector is a read-only script. It reads:
  - globals through data descriptors only, so no getters run
  - `querySelector` hits
  - React/Vue root markers
  - resource-timing URLs and script URLs, without query strings
  - the first 2 KB of up to 20 inline scripts
  - the generator meta tag
  - visible text length

  Output is capped at 256 KB and re-capped in Rust (`parse_rendered`).
- New signal kinds:
  - `js`: a global path or `@marker`
  - `dom`: a selector with an optional attribute and version

  The static scan never fills these inputs. js/dom signals were added for Next.js, Nuxt, Vue, Angular, Svelte, Gatsby,
  Remix, Astro, React, jQuery, Shopify, GA, GTM, WordPress and Microsoft Clarity.
- Merging only adds evidence, so confidence never falls. A hydrated page drops `SpaShell`, and a passed bot check drops
  `Challenge`.
- Skipped during meetings and for plain `http` sites (with a note).

### Mouse-free tour (`reverse_scroll.rs`, runner)

- A `ViewportStepper` is made fresh per viewport, at most 2 steps (Moved / Retry / LastView / NoMove / Stalled). The old
  shared controller ended the whole tour after 12 wheel bursts in total.
- Scrolling:
  - It uses UI Automation `ScrollPattern` on the browser's Document (no mouse, no keys, no focus change), with the vertical
    scroll percentage for end detection.
  - If the browser does not offer that, it falls back to PageDown through the shortcut router, which re-checks the window.
  - `move_eased` and mouse-wheel input are gone from the tour.
- The tour stops with "You took over, so I stopped the tour" when:
  - the foreground window changes, or
  - `GetLastInputInfo` shows input after the tour started that is not our own PageDown (`user_took_over`, wrap-safe ticks,
    plus a new `memcore::resume::last_input_tick`).
- Shift is measured on the Document rectangle (`measure_region`). The tab strip and address bar no longer fool the estimator.

### Sidebar (`StackView.tsx`, `stackTypes.ts`, `sidebarStore.ts`, `stack.css`)

- A new scan clears the previous progress strip and lit cards.
- The "why this scan is limited" list, a "Looking deeper in a hidden browser window…" line while rendering, and versions
  next to names.
- App scans:
  - say "App stack" and "In this window"
  - show the exe name as-is
  - hide the meaningless security-header and page-weight cards
- The report ends with a privacy footnote listing what left the machine:
  - one anonymous fetch
  - the hidden render
  - screenshots sent to the user's Gemini key
  - tech domains sent to Logo.dev

### Accuracy harness (`site_eval.rs`, public; `dev_tools/site_capture.rs`)

- `cargo run --example site_capture -- <url>` saves one guarded fetch as `tests/fixtures/site_real/<host>/page.json` plus a
  labels template. This is owner-run and needs the network.
- `site_eval::tests::real_site_accuracy` replays every fixture offline. It prints per-category precision, recall and F1,
  false positives, misses and calibration. It fails on:
  - a headline-category F1 drop over 0.02 against `baseline.json`
  - any must-not-find
  - a lost expected thin reason
- Four **synthetic** `.example` fixtures exercise the path (Next.js SSR + render, WordPress/WooCommerce, Cloudflare challenge,
  Vite SPA + render). They prove the harness works and say nothing about real-site accuracy.

## Verified (run here)

- `cargo check --lib --tests`: clean. The only warnings are 2 pre-existing ones in `desktop_control.rs`.
- Focused Rust tests: 140 passed, 0 failed.
  - `site_probe`, `site_render`, `site_eval`, `reverse_engineer`, `reverse_scroll`, `app_probe`, the reverse-prompt tests
    and `test_reverse_engineer_phrases`
  - Two failures found on the first run were fixed: the indirect phrasing, and a reachable "found 0 technologies".
- Frontend: `tsc --noEmit` clean; sidebar vitest 21 files / 143 tests (4 new).
- Full Rust library suite (serial): 3,221 passed, 0 failed, 35 ignored.
- Full frontend vitest: 107 files, 1,063 tests passed.
- 8 safety mutations, each killed by a test:
  - tour allowed after a refusal
  - `guard_url` dropped from the render navigation policy
  - popups allowed
  - `fetch(` added to the collector
  - private-IP request filter removed
  - implies floor removed
  - "found 0 technologies" closing line
  - own-input slack in `user_took_over`
- Gates:
  - turn-end, repo hygiene, IPC contract and window opacity pass.
  - The dead-code ratchet lists 2 items in `desktop_control.rs` (field 0, method `clear`). They come from other in-progress
    work, not this change. `live::commands::mouse::scroll` lost its last caller here and was deleted.

## Real-site check (2026-11-14, static scan only)

The scanner's own guarded fetch was run once against 10 public sites whose stacks are public:

- nextjs.org
- wordpress.org
- react.dev
- svelte.dev
- nuxt.com
- astro.build
- allbirds.com
- angular.dev
- vite.dev
- github.com

The tool was `cargo run --example site_capture -- <url>`, which now prints the analysis. `--replay <dir>` re-analyses saved
pages offline. The captures stayed in the session scratchpad and are **not** committed, because they are third-party pages.

**Headline stack: right on all 10.**

| Site | What was found |
|---|---|
| nextjs.org | Next.js + Vercel |
| wordpress.org | WordPress + nginx |
| react.dev | Next.js + Vercel + GA |
| svelte.dev | SvelteKit + Vercel (after the fix below) |
| nuxt.com | Nuxt + Vue + Vercel |
| astro.build | Astro + Cloudflare |
| allbirds.com | Shopify + Cloudflare |
| angular.dev | Angular + Fastly |
| vite.dev | Vue + Netlify |
| github.com | Turbo + GitHub |

**False positives found and fixed.** The first pass produced about 45 false positives from loose patterns:

- Semantic UI from any `ui ` class
- Bulma from any `is-` class
- Ant Design from "important-"
- "Markdown Content" from any `.md` link (entry removed)
- Zustand from `create(`
- shadcn/ui from `cn(`
- Django from `X-Frame-Options: SAMEORIGIN`
- Rails and Heroku from `X-Request-Id`
- AngularJS (Detected!) on angular.dev from the text "ng-app"
- Ghost from `ghost-` classes
- Leaflet from `L.map` in minified code
- Remix from any `/build/` stylesheet
- Turbo from Next.js's `turbopack-*.js` chunks
- Hotjar from `hj(` in minified code
- Bootstrap from `.btn-`
- Iconify from `data-icon`

Each signal became a supporting hint only, or a tag-anchored or word-bounded regex. A replay of the same saved pages then
showed none of them, while correct detections stayed (Turbo on github.com via `data-turbo`).

**Miss found and fixed: SvelteKit.** The table looked for `__SVELTEKIT__` and `/_app/immutable/` in script tags. Current
SvelteKit writes:

- `__sveltekit_<hash>`
- `data-sveltekit-*`
- `/_app/immutable/` in link tags

svelte.dev is now SvelteKit at 100% and Svelte at 92%, from its `svelte-xxxxxx` scoped classes.

**Regression tests:**
- `ordinary_markup_and_minified_code_name_no_technology`
- `current_sveltekit_markers_are_detected`

The `implies` pass is now its own `apply_implies` and is tested directly.

**Sidebar preview (dev harness `/dev-stack.html`, dark and light).** The bot-check notice, the "looking deeper" line and the
app view render with no console errors. The preview exposed two fixes:
- App scans no longer show "N scripts · N third-party hosts".
- Section numbers start at 1. They used to show the 0-based index, on website tours too.

Still unmeasured: the hidden render on these sites (it needs the app), and precision and recall against hand-labelled
fixtures. This check is a by-eye review of 10 sites, not a scored benchmark.

## Build (2026-11-14, owner-approved)

- Personal release **`a3d6200b246d9b4a`**: `node nexus.mjs build --profile personal`, 10m 12s, features `custom-protocol`
  (matching the previous personal build).
- Provenance passes, checked with `verifyBuild` on `target-personal/release`: the source hash matches the tree, and the
  executable hash matches. The CLI `build-provenance.mjs verify` only checks the legacy `target/release` folder, so it
  reports that older build as stale.
- The new runner strings are in `nexus.exe`. The new sidebar text is in `frontend/dist`.
- The previous running instance (PID 10544) was stopped at the owner's request. It ignored a normal close and was forced.
- The build includes the other uncommitted work in the tree.
- Not run live yet.
- **First live attempt (23:40–23:43 IST): no run reached the feature.**
  - Launched as PID 4268. The output was piped to a log, because a release build cannot write to the console handle the
    launcher passes it.
  - The startup self-check passed all 5 areas, and the Gemini key is present.
  - The Intel SST microphone delivered exact digital silence from about 1 s after start: the highest level it registered
    was 0.003. NEXUS restarted the stream twice on its own without effect.
  - A direct 2 s capture outside NEXUS read rms 0.000015.
  - No wake, hotkey press or transcript was logged, so no `[REVERSE]` line exists yet.
  - Windows microphone privacy is Allow, and the device status is OK.
- **Second live attempt (23:46 IST, after the owner fixed the mic): two real defects found.**
  - The wake word fired (stage-1 0.874, confirmed by STT) and the owner was verified (0.97). The commands were transcribed
    correctly.
  - **"Let's reverse engineer this website."** was dropped by the transcript filter's dictionary gate (`[STT-DROP] …
    dict_gate`). The parser did not accept a leading "Let's", and the gate only passes unknown-looking text that parses.
    Fixed: the lead-ins now include "let's"/"lets" and "go (and)". Tests cover both the phrase table and
    `test_real_commands_survive_the_filter`.
  - **"Reverse engineer this website."** (spoken, then again via Ctrl+Space) parsed to `NluResult reverse_engineer` and was
    routed to GhostCenter/LocalCommand. It then emitted Done in 0.3 ms without running anything.
    - Root cause, present since change 152: `orchestrator.rs` has a normal-mode dispatch table and a Ghost-session table.
      Only the Ghost table had a `reverse_engineer` arm, so outside Ghost Mode the feature never ran.
    - Fixed by adding the arm to the normal table.
    - Pinned by `orchestrator_dispatches_reverse_engineer_in_normal_and_ghost_mode`, which checks both tables call
      `reverse_engineer::run`.
  - Both fixes need a rebuild. Build `a3d6200b246d9b4a` still has both defects.
  - Wider gap, noted and not fixed: any NLU label that `center.rs` accepts but `orchestrator.rs` has no arm for finishes
    the same silent way.
- **Rebuild with both fixes:** personal release **`bb04c8bb77ba1441`**, 9m 19s.
  - Provenance passes, and the source hash matches the tree that contains both fixes.
  - Running as PID 18304 since 00:06 IST.
  - The startup self-check passed all 5 areas.
  - Live voice run pending.

- **Third live attempt (00:08 IST, build `bb04c8bb77ba1441`): the command now runs, the address read fails.**
  - Both "reverse engineer this website" attempts reached `reverse_engineer::run` (the normal-mode arm works).
  - Every stage of the address ladder returned nothing (verified, 3 retries, relaxed, Ctrl+L), so NEXUS said it could not
    read the tab's address. Ten seconds earlier the notification code had found a full-screen window in front, so the
    likely cause is a browser in full screen (F11 or a video's full screen), where the toolbar and its address bar are
    not on screen.
  - Separately, Brave names its location bar `BraveLocationBarView`, and the first lookup matched `LocationBarView`
    exactly. The fallback (any edit field named "Address and search bar") still found it in a normal window, so this was
    a slow path, not the failure.
  - Fixed in source:
    - The lookup accepts any `*LocationBarView` (`is_location_bar_class`).
    - A full-screen browser leaves full screen with F11, the address is read, and F11 puts it back. The second F11 is sent
      only if the first really left full screen and the same window is still in front (`read_out_of_fullscreen`).
    - When every stage fails, one `[REVERSE] url diag:` line names the foreground program, whether it is full screen,
      whether a location bar exists, and the edit fields with their accessible names (never their values).
  - Known side effect: a page in a video's own full screen comes back in browser full screen (F11), not the video's.
- **Fingerprints from open-source projects (same day).**
  - Licences checked from the upstream files: Wappalyzer's definitions and every maintained fork (`enthec/webappanalyzer`,
    `HTTPArchive/wappalyzer`, Censys) are GPL-3.0. They were not used, because NEXUS ships no GPL code or data (the same
    reason espeak was removed in change 81).
  - Imported instead, both pinned to a commit, both credited in `THIRD_PARTY_NOTICES.md` with their licence files in
    `src-tauri/src/fingerprint_licenses/`:
    - **Retire.js** (Apache-2.0): script file names, script URLs and script banners for about 70 libraries, each with a version
      capture. Patterns are converted to Rust regex syntax; ones with backreferences or lookaround are dropped. The
      vulnerability data is not imported.
    - **Library Detector for Chrome** (MIT): page globals that only one library sets, read by the hidden render pass,
      with a version path where there is one. Only plain `win.<path>` checks are converted; its detection functions are
      not copied or run.
  - `scripts/import_fingerprints.py` does the conversion and can be run again: it removes what an earlier run added
    (imported evidence ends with "[Retire.js]" or "[Library Detector]"; imported technologies carry `source`) and never
    touches hand-written signals.
  - Result: 243 to 364 technologies, 451 new signals, 45 existing technologies gained version-capturing signals. New
    category "JavaScript Library".
  - Offline check on the 10 real sites captured on 2026-11-14 (`site_capture --replay`, old table against new): no new
    false positives and no lost detections. React went from "Likely, implied by Next.js" to "Detected" with its version
    on react.dev (19.0.0) and nextjs.org (19.3.0), and Next.js gained its version on react.dev (15.1.12), all from
    Retire.js banners. The Library Detector signals need the hidden render, which this replay does not run.
  - Guards against false positives: generic globals (`$`, `_`, `React`, ...) never name a library alone; Underscore's
    `_.VERSION` is skipped because Lodash sets it too; jQuery UI widgets, dojox, LitElement and lit-html merge into
    their parent technology.

## NOT verified (needs the owner, fresh release)

- Any live voice run:
  - Gemini latency per viewport
  - window move and restore
  - UI Automation `ScrollPattern` on Brave, Chrome and Edge (PageDown fallback if absent)
  - the "you took over" stop
  - meeting mode
- The hidden render on real WebView2:
  - whether the 8×8 off-corner window hydrates pages normally
  - whether `incognito(true)` and the `WebResourceRequested` filter behave as written
  - RAM during a render (expected about 250 MB, freed on destroy)
  - the COM calls compile but have never executed
- Real-site accuracy: no real site was fetched here. Capture the 50-site set (audit §7) with `site_capture`, label it, and
  replace `baseline.json`.
- Known leftover risk: WebView2 resolves DNS itself, so a public name that rebinds to a private address is not caught by the
  request filter. This is documented in `site_render.rs`.
