# 03 — "Nexus, reverse engineer this website/app": how it works and how good it is

Audit date 2026-11-01. Read-only: no source was edited, nothing was built or launched, and no third-party site was scanned.
Labels: **[code]** = verified by reading the code (file:line given), **[run]** = I ran it, **[inferred]** = my reasoning or public knowledge that I did not test here.

## 0. Short answer

It works end to end in code and has a clean, safe design. It is **not perfect, and nobody has measured it**. The only accuracy evidence is 6 hand-made HTML fixtures of 248–769 bytes each. No real site has been scanned in a test, and the owner walkthrough listed in change 152 (one Next.js site, one WordPress site) has not been done. The fingerprint table has 267 entries. Wappalyzer-class tools have well over 3,000. Several structural gaps (section 3) mean modern React/Vue/Vite single-page apps are often reported as "nothing found" or as only their CDN.

## 1. The full path

### 1.1 Voice trigger [code]
`intent_parser.rs:947-950` calls `parse_reverse_engineer_command` (`intent_parser.rs:3950-3990`). The result is the NLU label `reverse_engineer`, which `center.rs:82,597` accepts and `orchestrator.rs:1733-1735` sends to `reverse_engineer::run(app, prompt)`.

Recognised phrasings (the patterns at `:3962-3984`):
- The verb: `reverse engineer …`, which also catches the STT mishearing "reverse and gineer". `reverse engineer this` with no object also triggers.
- `what/which … tech(nology) stack|technologies|stack|framework(s)|libraries|cms … this|the|that site|website|page|web app|app|application`
- `what is/was this site built|made|created|developed|coded with|using|on|in`
- `how was/is this site built…` and `how did they|someone|people build|make|create|code this site`
- `break down|tear down|dissect|decompile|deconstruct this site`
- `analyse|scan|inspect|check … site … stack` (in either word order)
- Refused when the sentence names repo, PR, github, function, file, binary, exe, firmware or malware (`:3955`). Negation ("please do not…") is refused, per the test at `:10610`.

Tests: `test_reverse_engineer_phrases` (`intent_parser.rs:10573-10616`) covers 16 positive phrases and 9 guard phrases.

Likely misses [inferred from the regexes, not tested]:
- "what's this site built with": the pattern needs `what\s+(is|was)`, and `review_command_candidate` (`:3758`) does not expand "what's".
- "what framework is this?" with no site word.
- "is this WordPress?"
- "what's this made of"
- "what CMS does this use"

Likely false trigger [inferred]: "what technologies does the app store use" (pattern 2).

### 1.2 Runner — `reverse_engineer.rs::run` (Windows, `:600-1154`) [code]
1. `screen_context::collect_target` and `capture_block_reason` stop the run on sensitive windows (`:624-630`).
2. Target choice (`classify_target`, `:36-49`):
   - A URL is available → Website.
   - The foreground app is a browser but its URL cannot be read → NeedUrl. NEXUS says "Click the address bar once and ask again" (`:680-688`).
   - Otherwise → App, using the **foreground process's exe path**.
3. Website path:
   - The probe is spawned at once (`:745`).
   - The browser window is moved right to make room for a 540 px left sidebar (`plan_page_rect`, `reverse_scroll.rs:13-21`).
   - The `stack` sidebar shows "scanning" (`:771-774`).
   - The report is painted, then a one-sentence summary is spoken (`:786-829`).
   - Logos are fetched later as a patch, through `brand_logo` / Logo.dev, with a 6 s timeout (`:802-821`).
4. Tour: up to `max_viewports(prompt)` screens. That is 2 for quick/brief/short/fast, 6 for full/detailed/thorough/everything/whole/complete/entire, and 4 otherwise (`reverse_scroll.rs:222-245`). Each viewport goes through these steps:
   - capture the page, excluding the stage overlay (`:1157-1176`)
   - send the image to Gemini through `screen_tour::analyze_reverse_image` with `build_reverse_prompt` (`screen_tour.rs:324-379`)
   - narrate it with drawings (`narrate_tour_script`)
   - light up the matching technology cards (`reverse:focus_card`, `orchestrator.rs:4751-4766`)
   - move the **real cursor** to the page centre and scroll with the wheel (`:946-997`), measuring the shift between grayscale frames (`estimate_scroll_shift`). The tour stops at AtEnd or Stalled, or after 12 wheel steps.
5. App path: `app_probe::probe` runs on a blocking thread, followed by **one** captured frame that is narrated (`:1001-1101`).
6. Cleanup: the window is restored (maximised or put back at its old rect), the final payload is sent, and the closing line "Walkthrough complete, sir. I covered N sections and found M technologies" is spoken (`:1103-1153`).
7. During a meeting the run is quiet: no window move, no speech, no tour, only the sidebar (`:694-699, :852`).
8. On non-Windows systems NEXUS says "Reverse engineering needs Windows for now" (`:581-598`).

### 1.3 `site_probe.rs` — the website scanner [code]
- **Fetch** (`:1228-1312`):
  - One anonymous GET, with cookie names collected but no cookies sent.
  - Up to 5 redirects followed by hand, each hop re-checked by the guard and DNS-pinned (`:1034-1069, :1238-1254`).
  - HTML capped at 1.5 MB, 20 s total timeout.
  - It then fetches up to 6 **same-site** external scripts and 2 stylesheets (any host, still guarded), each capped at 400 KB with an 8 s timeout (`:1176-1226`).
  - The UA is `… NEXUS-StackScan/1.0` (`:36`). It never runs JavaScript.
- **SSRF guard** (`:912-1023, :1034-1052`):
  - Refused: http(s) only, userinfo in the URL, localhost/.local/.internal/.lan/.home.arpa, and private, loopback, CGNAT, link-local, documentation, benchmark, multicast and reserved v4 ranges.
  - IPv6: refused for ULA, link-local, multicast and documentation ranges. Mapped, compatible and NAT64-embedded v4 addresses are judged by the embedded v4 address.
  - **Every** resolved address must be public, and the client is pinned to the checked IP (this blocks DNS rebinding).
  - Sensitive (banking/private) URLs are refused through `screen_context::is_sensitive`.
  - This is a solid guard. Remaining gaps [inferred]: `pick_subresources` re-guards each sub-URL, but a redirect inside a sub-resource fetch is never followed (`Policy::none`), which is safe. IPv6 6to4 (`2002::/16`) and Teredo (`2001::/32`) embedded v4 are not decoded. They are low risk because their outer address is public.
- **Matching** (`analyse`, `:480-684`): regex scanning of tags only, with no DOM parser. Signal kinds:
  - `html` (searched in lowercased HTML)
  - `header`, `cookie` (name prefix), `meta` (name/content)
  - `script_src`, `link_href`
  - `script_text` (fetched external scripts only)
  - `css_text`
  - `url` (host)
- **Confidence**: noisy-OR, `1 - Π(1-w)` (`:282-285`).
  - Detected ≥ 0.80, Likely ≥ 0.50, Inferred ≥ 0.30. Anything below 0.30 is dropped (`:51-61, :600`).
  - Signals with weight < 0.30 are "support only". They add evidence text but **never add confidence** (`:174, :591-596`).
  - `implies` adds the implied technology at 0.8 × the parent's confidence (`:618-643`).
- **Also reported**: presence of 6 security headers (HSTS, CSP, XFO, XCTO, Referrer-Policy, Permissions-Policy; presence only, not quality), page counts, up to 30 third-party hosts, title, description and lang. The markdown report is built at `:830-896`.

### 1.4 `site_fingerprints.json` [run: counted with a Python script]
- version 1, **267 technologies, 570 signals**, 33 categories.
- Largest categories: frontend-framework 24, build-tool 21, analytics 20, cms 18, security 15, state-management 14.
- Small categories: fonts 2 (Google Fonts, Adobe Fonts), cdn 5, tag-manager 1, video 2, search 3.
- Signal kinds: script_text 214, script_src 127, header 83, html 65, cookie 33, link_href 25, url 12, meta 9, css_text 2.
- Weights: 150 signals are < 0.30 (support only), 16 are 0.30–0.5, 155 are 0.5–0.8, and 249 are ≥ 0.8.
- **25 technologies can never be detected**, because all their signals are support-only: rollup, redux, elementor, elasticsearch, typescript, eslint, prettier, jest, vitest, playwright, cypress, storybook, formik, swr, pinia, vuex, xstate, wouter, remark, rehype, unified, sharp, imagemin, heroicons, phosphor-icons. Only redux is reachable, through `implies`.
- **Duplicate entry**: `clarity` and `microsoft-clarity` are both named "Microsoft Clarity". The "no duplicates" check in change 152 compares ids only, so one site would show two cards. `microsoft-clarity` also matches any script URL containing the bare word "clarity" at weight 0.9, which is a false-positive risk.
- No version detection (3 entries mention "version"). No entry for DNS, TLS certificates, robots.txt, or JS globals.
- How current it is: hand-written for change 152 (2026-10-21) and never refreshed. There is no update channel; it is `include_str!` into the binary.

### 1.5 `app_probe.rs` — the desktop scanner [code]
- It reads the **install folder of the foreground exe**: up to 3,000 names, a bounded depth, symlinks skipped (`:225-268`), and the Squirrel `app-x.y` / `bin` layout is handled (`:285-307`).
- It also reads the first 4 MiB of the exe as text (`:271-283`) and the version resource (company, product, version; `:149-220`).
- It detects:
  - frameworks: Electron, Tauri/WebView2, WebView2, CEF, Qt, Flutter, WPF/.NET, Java, Python/PyInstaller
  - runtimes: Node, Go, Rust, Delphi, AutoHotkey
  - installers: Squirrel, electron-builder, MSIX, Inno Setup, NSIS, ClickOnce
  - engines: Chromium/Gecko, Unity, Unreal
- System paths are refused. It never executes anything, injects code or reads memory.
- Limits:
  - It only sees the foreground app; there is no "pick a file/installer" option.
  - Matching is substring-on-filename (`has_file` = `contains`, `:317-321`) and substring-in-exe-head, so "electron" or "tauri" anywhere in the first 4 MiB counts. False positives are possible [inferred].
  - The Python rule is `python3*` plus any `.dll` (`:625`), which is loose.
  - It does not read `resources/app.asar` / `package.json`, so it cannot say which JS framework an Electron app uses.
  - It does not parse PE imports, .NET metadata, or Store/WindowsApps apps (access is usually denied).

### 1.6 `reverse_scroll.rs` [code]
Pure logic, 503 lines:
- page rect planning (minimum work area 1100 px, minimum page width 640 px)
- the notch controller with AtEnd/Stalled decisions
- a 96-column grayscale row-shift estimator
- the viewport phrase table

### 1.7 Frontend `StackView.tsx` (638 lines, test file `stackView.test.tsx`) [code]
- Grouped or by-confidence lists, with an evidence list per technology on expand.
- Confidence tags, plus a security score computed from the 6 header flags.
- Copy markdown and download `.md` are **already built** (`:49-80, :193-204`).
- Cards are focused from the tour (`reverse:focus_card`).

## 2. Tests

[run] Command: `cargo test --lib -j 1 -- site_probe reverse_engineer app_probe reverse_scroll intent_parser::tests::reverse` (offline). Result: see §2.1.

What the tests cover [code]:
- **site_probe**:
  - the confidence math
  - the IPv4/IPv6 tables, including the disguised-v4 regression test
  - the URL guard table
  - header, cookie, implies, threshold and dedupe behaviour
  - category order and markdown
  - 6 `analyse` fixtures: Next.js+Vercel, WordPress+WooCommerce, Shopify, Nuxt+Cloudflare, static Tailwind+GA, Angular
  - a local mock server for 200, non-HTML, truncation, private redirect hop, too many redirects and hang→timeout
- **app_probe**: Electron, Tauri, Qt, Flutter, WPF/.NET, Unity, PyInstaller, empty folder, mixed stacks, temp-folder probe, system path refused, missing path, the 3,000-entry cap, no symlinks, markdown.
- **reverse_engineer**: payload builders, logo patch, summary and closing lines, restore and progress phases.
- **reverse_scroll**: rect plan, controller, shift estimator, phrase table.

**The fixtures are synthetic.** The 6 files in `src-tauri/tests/fixtures/site/` are 248–769 bytes, and their signal strings were picked to make the tests pass. The comment "analyse tests from real fixtures" (`site_probe.rs:1640`) is misleading. They prove the matching engine works. They do not show accuracy on real sites.

### 2.1 Run result [run, 2026-11-01]
**82 passed, 0 failed, 0 ignored** (2,842 others filtered out), 4.93 s:

| Module | Tests passed |
|---|---|
| site_probe | 28 |
| reverse_scroll | 21 |
| reverse_engineer | 17 |
| app_probe | 15 |
| intent_parser `test_reverse_engineer_phrases` | 1 |

The fingerprint engine was also checked on the repo's saved fixtures. They pass, but they are synthetic, so this run does not measure real-site accuracy.

## 3. Accuracy: structural false negatives and false positives [code + inferred]

1. **Inline scripts are not searched by `script_text`** (`site_probe.rs:461` uses only fetched external scripts). 214 of the 570 signals (38 %) never see inline `<script>` bodies, where most analytics snippets, `window.__NUXT__`, hydration state and config objects live. Only the 65 `html` signals scan the whole page.
2. **Only same-site scripts are fetched** (`:1194`). Framework bundles served from a CDN host (a different registrable domain) are matched by URL only, never by content. Many sites ship `/assets/index-abc123.js` (Vite) with no telltale URL, so the content check is the only clue, and it is limited to 6 files of 400 KB each.
3. **No JS execution and no DOM after hydration.** Client-rendered SPAs (Create React App, Vite+React/Vue/Svelte) serve an empty `<div id="root">`. Without globals such as `React.version`, `__VUE__` or `ng.version` (Wappalyzer `js` keys) or DOM checks (`dom`), these sites come back mostly empty.
4. **React 18+ is nearly invisible** unless Next.js is detected. React's only strong signal is `data-reactroot` (weight 0.85), which `createRoot` no longer writes [inferred, React 18 behaviour]. The `react.production.min.js` regex is a `script_text` signal, so it searches script *content* for a filename, which bundles rarely contain.
5. **Anonymous fetch ≠ what the user sees.** The probe sends none of the browser's cookies, so login-gated apps return their login or marketing page. Behind a WAF or bot wall (Cloudflare challenge, Akamai, DataDome), the probe receives a challenge page: it then reports "Cloudflare" and little else. The custom UA string makes bot walls more likely [inferred]. The vision tour, which sees the real tab, can therefore describe a different page from the one the stack was taken from, and nothing tells the user.
6. **`same_site` uses the last two host labels** (`:374-391`). Every `*.co.uk` / `*.com.au` host counts as "same site": over-fetching and wrong third-party lists. There is no public-suffix list.
7. **`implies` ignores its own "confirmed only" comment** (`:618`). An Inferred 0.30 parent adds an implied child at 0.24, below the drop threshold but still shown as "Inferred".
8. **Single-signal technologies at ≥ 0.9 with loose substrings** (for example `microsoft-clarity: script_src contains "clarity"`) can fire on unrelated URLs.
9. **Fonts, API style and build tool are weak.**
   - Fonts: only Google and Adobe; self-hosted `@font-face` fonts are ignored.
   - API style: no GraphQL endpoint, REST, tRPC or WebSocket detection; `apollo-graphql` only matches strings.
   - Build tools: most are support-only (rollup, TypeScript) or rely on bundle-content strings that are mostly not fetched.

What it does well: header-based hosting and CDN detection (`cf-ray` 0.99, `x-vercel-id`, `server`), hosting by host-name suffix (`*.vercel.app`, `*.pages.dev`), cookie-based CMS and e-commerce (WordPress, Shopify, WooCommerce), meta generator tags, and Google Fonts. On server-rendered or CMS sites it should be reasonable [inferred].

### 3.1 Honesty and unsure handling [code]
- Statuses are honest: Detected, Likely and Inferred, with per-tech evidence strings, and the summary says "I found N technologies; K are confirmed" (`reverse_engineer.rs:385-440`).
- The spoken head says "This looks like X" even when X is only Inferred (0.30), and it does not soften the wording by confidence.
- When nothing is found it says "I couldn't identify the technologies from outside" (`:386-388`), and the markdown says "Not detectable from outside".
- The vision prompt tells Gemini never to present a guess as fact and to say "likely" (`screen_tour.rs:365-366`). `stack_refs` are filtered to detected ids (`screen_tour.rs:776-789`).
- Gaps:
  - A refused or failed scan still runs the full tour and still ends with "Walkthrough complete … found 0 technologies" (`:842-847, :1000, :1146-1148`).
  - The tour prompt says "this website" even in App mode.
  - Nothing tells the user that the scan saw a login page or a bot challenge.

### 3.2 Scrolling and window handling [code + inferred]
- Moves the user's real cursor to the page centre and sends wheel notches (`:950-966`). That works if the page is under the cursor, but it fights the user if they move the mouse. It may scroll an inner scroll container, a map or a carousel instead of the page [inferred].
- Smooth-scroll pages, sticky headers, lazy-loading and infinite scroll can confuse the row-shift estimator; the tour then stops early (Stalled) or never reaches the footer [inferred]. Not tested live (change 152 "NOT verified").
- Up to 6 Gemini vision calls with 20 s timeouts each: a full tour can take minutes [inferred].
- Privacy: detected technology domains plus the scanned site's domain are sent to Logo.dev for icons (`:802-821`) [code]. Screenshots go to Gemini. Both are acceptable for a BYOK assistant, but users are not told.

## 4. State of the art comparison [inferred, from public docs; licence verified via the GitHub page]

| Tool | Technologies | Methods | NEXUS gap |
|---|---|---|---|
| Wappalyzer (closed since 2023) / **webappanalyzer** (enthec fork) | ~3,000+ (not counted here) | headers, cookies, html, `scriptSrc`, `scripts` (content), meta, **js globals**, **dom selectors**, **dns**, **certIssuer**, css, `xhr`, robots, url, `probe`; `implies` / `requires` / `excludes`, versions via `\;version:` | 11× coverage; JS globals, DOM, DNS, TLS, XHR, versions, `excludes` |
| WhatRuns (extension) | proprietary | runs inside the real browser tab (sees the logged-in, hydrated page) | NEXUS fetches anonymously, outside the browser |
| BuiltWith | tens of thousands, historical | crawl plus history, relationships, spend | not comparable (commercial crawler) |
| httpx `-tech-detect` (ProjectDiscovery, uses `wappalyzergo`) | Wappalyzer-derived set | headers + body, with the fingerprints bundled | the same open dataset idea, in Go |
| Nuclei `technologies/` templates | hundreds of templates | active probes (paths, favicon hash, specific endpoints) | NEXUS never probes paths (good for safety, less recall) |

**Can webappanalyzer be reused?** Its licence is **GPL-3.0** (verified on github.com/enthec/webappanalyzer).
- Shipping its JSON inside the NEXUS binary or installer makes the distributed work subject to GPL-3.0 copyleft obligations [inferred legal reading; get a proper licence review].
- Options:
  - (a) Keep NEXUS's own table and use webappanalyzer only as a *reference* for which signals exist. Facts such as "header `x-powered-by: Next.js`" are not copyrightable, but copying the curated JSON wholesale is risky.
  - (b) Make NEXUS (or the scanner) GPL-compatible.
  - (c) Download the dataset at runtime as a user-chosen optional add-on, kept separate from the binary. Mere aggregation arguments apply; still get advice.
  - (d) Older MIT-licensed Wappalyzer snapshots (before August 2023) exist in public forks. Their provenance and staleness need checking.
- How it would plug in: the schema maps almost one-to-one onto the existing kinds.
  - `headers`→`header`, `cookies`→`cookie`, `meta`→`meta`, `scriptSrc`→`script_src`, `scripts`→`script_text`, `html`→`html`, `url`→`url`, `implies`→`implies`.
  - Needed additions: a converter for Wappalyzer's `pattern\;confidence:50\;version:\1` syntax (confidence → weight); new kinds `js` and `dom` (need a rendered page, see improvement 2); `dns` and `certIssuer` (need DNS TXT/MX lookups and TLS issuer, both doable in Rust); `excludes` / `requires`.
  - `site_fingerprints.json` would be generated, not hand-edited, and the existing table check would be kept as a CI gate.

## 5. Verdict

**Not perfect.** Measured (by reading or running):
- the safety design (the SSRF guard is good and regression-tested)
- the pipeline wiring
- the engine's maths
- 6 synthetic fixtures
- the unit tests (§2.1)

**Unmeasured:**
- real-site precision and recall: none at all
- live voice runs, scroll reliability, window restore, narration quality and latency (the change 152 owner checklist is still open)
- app_probe on real installed apps

On SSR/CMS sites (WordPress, Shopify, Next.js on Vercel) it will probably be right about the headline stack. On modern client-rendered SPAs, sites behind bot walls and logged-in apps it will often under-report. It is honest about "not found", but it does not say *why* it found little.

## 6. Ranked improvements (largest gain per effort first)

1. **Search inline scripts with `script_text`** (add inline `<script>` bodies to `ctx.scripts`). Small change, large recall gain. Add JSON-in-HTML state markers (`__NUXT__`, `__remixContext`, `self.__next_f`, `ng-version`, `data-v-app`, `__sveltekit`).
2. **Read the real tab, not an anonymous copy.** Through the browser (CDP on Chromium/Brave, or a small extension), read `document`, the JS globals (`React?.version`, `__VUE__`, `ng`, `jQuery.fn.jquery`, `Shopify`, `wp`), the hydrated DOM and the network resource list (`performance.getEntriesByType('resource')`). This fixes SPAs, login walls and bot walls at once, and keeps everything local. Keep the anonymous probe as the fallback.
3. **Labelled accuracy set + CI score** (test plan in §7) before claiming any number.
4. **Grow the table to ~1,000+ technologies**, by generating it from a licence-safe source (see §4), and add version capture.
5. **Say why a result is thin**: detect challenge pages (`cf-mitigated`, "Just a moment…", `__cf_chl`), login redirects and empty SPA shells, and say so ("The site showed me a bot check, so I could only see the hosting").
6. **Fix the small bugs**: the duplicate Clarity entry, the bare-word 0.9 `clarity` substring, the 25 never-detectable entries (give each a strong signal or drop it), `implies` from Inferred parents, a public-suffix list for `same_site`, the closing line after a failed scan, "website" wording in App mode, and "what's" phrasings.
7. **Scrolling**: scroll with the page's own APIs through CDP (`window.scrollBy`, `scrollHeight`), or the `PageDown` key to the focused page, instead of moving the user's cursor. Stop on user mouse input.
8. **app_probe depth**:
   - read `resources/app.asar` headers / `package.json` (Electron framework, React/Vue, versions)
   - PE import table and .NET assembly metadata
   - accept a file the user names ("reverse engineer the installer in Downloads")
   - word-boundary matching instead of substrings
9. **DNS/TLS signals**: MX (Google Workspace/M365), TXT (SPF includes reveal email/marketing tools), CNAME chain (Vercel/Netlify/CloudFront), certificate issuer. Cheap, safe and high-value.
10. Tell the user when logos go to Logo.dev, or add a toggle for it.

## 7. Accuracy test plan (to be run by the owner, offline-reproducible)

- **Set**: 50 public sites with *self-published* stacks:
  - vendor showcases: Next.js showcase, Vercel customers, Nuxt showcase, Astro showcase, Shopify stores, WordPress.org showcase, Webflow showcase, Gatsby showcase, SvelteKit sites
  - company engineering blogs that state their stack
  - the projects' own sites (nextjs.org, nuxt.com, svelte.dev, astro.build, wordpress.org …)
  - Spread: 10 CMS/e-commerce, 15 SSR meta-frameworks, 10 pure SPAs, 5 static/Jamstack, 5 behind Cloudflare challenge or bot walls, 5 login-gated apps (scored on the public landing page).
- **Capture once, replay forever**: a dev-only tool saves each site's final headers, HTML and the fetched scripts/CSS into `tests/fixtures/site_real/<host>/` (respecting robots and terms of service, one fetch per site). Tests then call `analyse` on the saved files, so CI never touches the network.
- **Labels**: `labels.json` per site, listing the technologies that must be found, may be found, and must not be found, with the source of each label (a URL of the self-statement).
- **Metrics**:
  - per category: precision, recall and F1 (headline categories: meta-framework/frontend, hosting, CDN, CMS, analytics)
  - calibration: the share of Detected/Likely/Inferred results that are correct, which should be roughly ≥ 95 % / ≥ 75 % / ≥ 40 %
  - the false-positive rate per site
- **Baseline vs. challengers**: the current table; inline scripts enabled; the CDP mode (run live, separately); a webappanalyzer-derived table (local experiment only, not shipped until the licence decision).
- **Gate**: CI fails if headline-category F1 drops by more than 2 points, or if any must-not-find label appears.

## 8. Eight new feature ideas in the same family

| # | Idea | Value | Risk |
|---|---|---|---|
| 1 | **Security header audit, spoken** ("Is this site secure?"): grade CSP/HSTS/XFO/cookie flags/mixed content/TLS version, give 3 spoken findings and put the detail in the sidebar. Half built (flags plus `securityScore` exist) | High, cheap | Must say "passive check, not a pentest"; no active probing |
| 2 | **Compare two sites** ("compare this site with stripe.com"): side-by-side stack, weight and security diff | High for builders | Two fetches; keep the same guard |
| 3 | **Open-source equivalents**: map each detected SaaS (Algolia, Intercom, Auth0, Mixpanel) to OSS options (Meilisearch, Chatwoot, Keycloak, PostHog) from a curated table | Medium-high, offline | The table needs curation; wording must not sound like endorsement |
| 4 | **"Rebuild this" starter**: from the stack plus the vision sections, produce a scaffold plan (`npx create-next-app` + Tailwind + components list) in markdown, or hand it to a coding agent through agent_watch | Very high (fits the agent features) | Copyright/design copying: generate structure, never copy assets or text |
| 5 | **Performance snapshot**: page weight, request count, third-party count, LCP/CLS from the real tab via CDP `PerformanceObserver`; spoken "heavy page, 4 MB of JavaScript" | Medium-high | Needs CDP; numbers vary between runs, so say "this load" |
| 6 | **Accessibility quick check**: run axe-core in the real tab (MPL-2.0) and speak the top 3 issues | Medium-high | Licence fine if loaded at runtime; never claim compliance |
| 7 | **API discovery (own tab, passive)**: list the XHR/fetch endpoints the page already called (REST vs GraphQL vs tRPC vs WebSocket) from the browser's resource timing, with no replay and no fuzzing | High for developers | Misuse risk: passive only, never call endpoints, redact tokens and query strings |
| 8 | **Installer/binary explainer**: "reverse engineer the installer in Downloads": identify the installer type (NSIS/Inno/MSI/MSIX/Squirrel), signer, embedded framework, and a dependency list from PE imports, without executing it | Medium | Must never run the file; malware samples: refuse files flagged by Defender, and read headers only |

Bonus: **export the report** already exists (copy/download markdown in `StackView.tsx`). The remaining work is a voice trigger for it ("save that report") and saving to the memory/Docs path.
