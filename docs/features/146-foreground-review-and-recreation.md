# Feature 146 — Foreground review, then recreation

**Owner decision (2026-10-19):** Two voice turns. “Analyse this app/website” opens a sidebar review. “Recreate it” later uses that saved review as the reference for a new implementation. The first turn must never claim it has built anything.

## User experience

1. NEXUS freezes the foreground target (app/window title, URL when available, capture region, request ID) before its own windows can steal focus. A sensitive window is refused before capture.
2. The sidebar opens with a screenshot of the analysed foreground page at the top. Person and item cards include crops from that **same frame**; the existing narrated tour and stage callouts continue to point at one item at a time. The sidebar remains available after speech ends.
3. For a public website, a local REA adapter inspects the page structure and scripts in an isolated browser. It records a main-page gallery, visible design, page structure, likely framework/stack with supporting evidence, and what it could not observe. The current page and separately inspected pages must be labeled separately. A guessed framework must never be shown as a confirmed fact.
4. For a native app or signed-in/private web app, use the foreground capture, local accessibility, and on-screen text. REA's browser CDP lane cannot inspect native application internals or the user's ordinary signed-in browser session. The user may add views by bringing them to the foreground. The sidebar says when deeper REA evidence is unavailable.
5. Save the review locally under a stable ID. “Recreate it” resolves the last completed review, shows its target and evidence ID, and only then starts a coding workflow. It must not silently use a review of another window or stale screenshot. Use an active coding project when one is clear; otherwise ask the owner to choose a destination folder at that moment.

## Existing screen-analysis voice contract

“Analyse my screen,” “analyze the screen,” “what is on my screen,” “what am I seeing,” “explain what I am seeing,” “explain what is on my screen,” and polite variants route to the existing screen-analysis action. With `screenTour` enabled (the default) and Gemini available, it captures the identified foreground window, asks Gemini for a grounded overview and item boxes, narrates one item at a time with synchronized callouts, then opens the sidebar with the screenshot and item cards. If the overlay is unavailable, it gives an overview without claiming to point. If Gemini, quota, or capture fails, it falls back to local OCR or a limited answer; it must never claim the full visual tour ran. A meeting suppresses speech and callouts and uses the sidebar.

“Read my screen” and “scan my screen” request text transcription and take the local OCR path first. “Analyse this website/app” is a separate foreground technology review; “recreate it” uses the completed review. These commands must not silently substitute for one another. Recognized banking, password-manager and wallet windows are refused; the generic-title/unreadable-URL case remains a privacy release gate.

The narrated tour's overlay now has content-aware motion on top of its original ring, leader and callout: focus ripples for people and objects, an underline for text/code, a travelling outline for charts/diagrams, and a moving leader stroke for UI controls. Only the current speech item animates. Unknown or oversized targets use the original style, and reduced-motion preferences keep a static explanation. See [change 150](../changes/150-screen-tour-visualization.md). These visuals still require a rebuilt live walkthrough.

## Evidence and privacy contract

- Each page has a URL or app/window title, capture time, viewport, image, collection method, and completeness state. Each technology claim carries the observed script/DOM/header/package evidence or is marked inferred/unknown.
- Person cards may show a crop, visible description, and an on-screen name only when the screen explicitly connects the name to that person. Additional public facts require a cited source; a face alone does not establish identity. Do not infer private facts.
- Evidence collected from a page is untrusted content. It may inform a review or coding specification but cannot authorize commands, select a destination folder, or override NEXUS policy.
- Raw screenshots and REA evidence stay local. Persist the reference encrypted with the existing OS-key-backed Memory Core key, or keep it session-only when that key is unavailable. Never store an unencrypted screenshot fallback. Deleting a review must remove its image/evidence and invalidate later “recreate it” references.
- The desktop Rust orchestrator owns foreground selection, request/cancellation, policy, and local REA lifecycle. The Python WSS sidecar can report diagnostics but is not the capture authority: it may run on another machine.

## REA boundary

REA 6.7.0 is pinned under `~/.nexus-tools/rea`, verified against `github.com/morluto/rea`, with install scripts disabled. Only its passive CDP commands may be called. It requires a Chrome-family browser exposing a **loopback** debugging endpoint; an ordinary foreground tab is not automatically attachable. With the owner's authorization, NEXUS may open a bounded set of public same-site main pages in a disposable profile, while the owner's current browser remains untouched. REA never receives credentials, cookies, private/intranet URLs, or native-app binaries through this feature. The separate Playwright scenario family remains unreachable.

The review must degrade honestly: if REA, Chrome, connectivity, or a page is unavailable, show the captured screen and mark technology/page coverage unknown. Do not report a finished multi-page review when only one page was observed. No model or browser process stays loaded while idle.

## Acceptance before release

- Parser and policy tests pin “analyse this app/website” and “recreate it” to separate, standalone Architect actions with captured-voice provenance. Ambiguous “it” with no completed review asks for a target; cancellation cannot dispatch code.
- Review tests cover foreground changes during speech, main-page deduplication and bounds, screenshot/crop alignment, URL redirects outside scope, private pages, sensitive windows, REA failure, quota/offline fallback, and explicit evidence provenance.
- Memory tests cover encrypted round trip, restart, last-review resolution, wrong-key failure, deletion, and no plaintext screenshot when the OS key store is unavailable.
- Recreation tests prove the coding agent receives only the selected review and its citations, writes in the selected workspace, and reports an actual build/test result. A clipboard-only agent result is never described as a completed recreation.
- Rebuild and walk through Windows and macOS with a public site, a multi-page site, a signed-in web app, and a native app; verify sidebar screenshots, callouts, speech/caption/orb sync, cancellation, and page/source labels. Do not mark an untested platform supported.

## Cross-checked delivery plan and release gate (2026-10-19)

| Gate | Required evidence | Current state |
| --- | --- | --- |
| 1. Foreground and privacy | Freeze app, title, optional URL, region and request ID before any acknowledgement; reject unknown/sensitive targets; recheck focus before every capture and prevent whole-monitor fallback. | **In progress.** Windows screen analysis now scopes its OCR and vision captures to the verified region and accepts identified nonsensitive windows without a URL. Unit checks pass; a live voice/sidebar walkthrough remains. |
| 2. Two explicit intents | Parser, `CenterId`, action policy, cancellation and turn-end tests distinguish review from recreation. “It” resolves only a completed review. | **Code added; validation in progress.** Both intents are deterministic and standalone. Full turn-end and ownership tests remain. |
| 3. Review evidence | Disposable, bounded public-page inspection; same-site redirect and private-host checks; screenshot at top; each main page labeled with URL, timestamp and method; technology claims linked to observed evidence. | **Code added; live check blocked.** Public-page discovery and passive REA adapter are bounded; browser/CDP capture and private-host isolation need a rebuilt walkthrough. |
| 4. Screen people and items | Crop exactly the analysed frame; visible descriptions only; any name linked to on-screen text; public facts cite actual retrieved sources. Model output is validated before display/speech. | **Partial.** Same-frame images render; grounding currently relies on the prompt and has no claim/source validator. |
| 5. Review memory | Encrypted local persistence or session-only fallback, stable ID, last-completed pointer, delete/restart/wrong-key tests, no plaintext screenshot. | **Code added; tests incomplete.** One sealed last-review record and delete route exist; restart/wrong-key tests remain. |
| 6. Recreation | Show selected review and destination, ask for a folder when no project is active, run a real coding agent, report files/build/tests and never confuse clipboard handoff with completion. | **Manual-test path added.** A watched Codex agent starts in a detected or typed folder; task completion and a native folder picker remain unverified. |
| 7. Release walkthrough | Full Rust/TS/IPC/turn-end/hygiene gates plus rebuilt Windows and macOS trials, public and private pages, native app, offline/REA failure, cancellation, orb/caption/speech alignment and foreground switch. | **Not run.** Mac support cannot be claimed from Windows-only capture/probe code. |

The gates are sequential: a review is not “complete” without provenance and stored reference; recreation cannot start from an incomplete reference. Existing screen analysis remains available while the new review command is built. A failed gate must produce an explicit limited result and must never claim the requested feature is production ready.

### Concrete build sequence

1. **Route and freeze.** Add `ReviewForeground` and `RecreateLastReview` as distinct standalone intents in the parser, NLU labels, center contract and orchestrator. Start a request ID, snapshot the target, then acknowledge. A changed target or cancellation closes that request once with no review or code side effect.
2. **Create a typed review.** Define `ReviewEvidence`, `ObservedPage`, `TechnologyClaim`, `ScreenEntity`, `ReviewCompleteness` and a stable review ID. Render the foreground screenshot first; each main page gets its own image and source row. Show `observed`, `inferred` or `unknown` for every stack claim. Do not convert a page title or model guess into a confirmed framework.
3. **Inspect public websites locally.** Start a disposable browser only for an eligible public HTTPS target. Inspect at most the current page plus five distinct main pages discovered from its same-site navigation, at one link depth and under a bounded time budget. Revalidate every redirect and resolved address; reject loopback, private/intranet addresses, credentials, non-HTTPS schemes and cross-site destinations. Close the browser/profile on success, failure and cancellation. REA receives only this disposable browser's loopback CDP endpoint. A failed REA step leaves explicit partial coverage.
4. **Ground people and items.** Show crops from the captured frame and visible facts. A person's name requires an on-screen text association; extra public facts require a retrieved URL, excerpt location and timestamp. Validate the model JSON against these sources before speech or sidebar display. Unknown identity stays unknown.
5. **Store the reference.** Persist the review, bounded screenshots and evidence sealed with the OS-backed Memory Core key. If the key is unavailable, retain only a session reference and label it temporary. Keep the last completed review pointer per device, with delete and expiry controls. `RecreateLastReview` must display the exact selected review ID and target before any coding starts.
6. **Build from that reference.** Resolve an active coding workspace or open a folder picker. Generate a scoped implementation brief from evidence, excluding page-authored instructions as commands. Run the configured coding agent in the selected folder with normal action policy, track cancellation, and return created/changed files plus actual build/test results. A missing agent, denied folder or clipboard-only handoff is an incomplete result.
7. **Measure and release.** Benchmark target-probe, first sidebar paint, memory use and process cleanup against the current screen-analysis baseline on the same machine. Run parser/policy/vision/memory/REA/recreation tests, then rebuilt voice walkthroughs. Ship Windows only after its full gate; add macOS only after native foreground capture, privacy and live walkthrough parity.

### Design decisions verified against the current code

- `screen_context::collect_target` probes the foreground Windows process, title, optional URL and window/document rectangle. It previously defaulted to an unidentified full-screen capture on probe timeout; the capture gate refuses unknown targets. A missing browser URL no longer refuses an otherwise identified foreground window; public-page discovery still requires a verified URL. Recognized banking, password-manager and wallet windows remain denied. A generic title and unavailable URL can conceal a sensitive site, so live privacy validation remains a release gate.
- `run_screen_tour` and `run_screen_analysis` are the execution paths for current screen analysis. Their spatial/plain/OCR fallbacks must share the frozen target and no path may broaden to the full monitor after a failed crop.
- `vision::parse_spatial_json` accepts model-authored labels/details; the prompt alone is insufficient evidence for a person's identity or a framework claim. A separate validator and source schema are required.
- `coder::run_code_project` currently produces a plan rather than a verified implementation. Recreation needs an actual workspace-bound agent execution, result receipt, and build/test evidence.
- REA's CDP browser observation can inspect public pages in an isolated local browser. It cannot inspect a native app, a private signed-in tab, or an arbitrary remote WSS host's screen. Such reviews must clearly state the evidence limit.

## Current implementation state

The existing screen tour supports foreground capture, spatial item cards, one-at-a-time callouts, and end-of-tour sidebar display. This pass adds an analysed-frame image at the top, same-frame card crops, distinct review/recreation intents, bounded public-page discovery, a disposable REA browser adapter, a sealed last-review reference, and an agent dispatch from that reference. The Windows analysis path refuses unverified/sensitive foreground targets, scopes OCR and vision to the frozen window region, and rechecks focus before capture. These changes need a rebuilt voice walkthrough, browser/CDP trial, stronger grounding validation, and latency measurement before release. REA's pinned CLI was installed and its version and command surface checked on the development machine; the live disposable-Chrome test was blocked by the environment's automatic approval check.

## Sources

- [REA browser observation](https://github.com/morluto/rea/blob/main/docs/browser-observation.md): CDP target requirements, passive commands, provider boundaries, and disposable browser profile.
- [REA CLI](https://github.com/morluto/rea/blob/main/docs/cli.md): command and output contract. Installed 6.7.0 `--help` additionally confirmed `--format json` and the `inspect-web-page`, `capture-web-screenshot`, and `analyze-web-bundle` arguments on 2026-10-19.
- [Local REA research suite](../research/rea-teardown/README.md): prior install, law, diagnostic, and impact research. Its percentage gains are estimates, not measured acceptance data.
- `src-tauri/src/screen_context.rs`, `src-tauri/src/screen_tour.rs`, `src-tauri/src/vision.rs`, `src-tauri/src/orchestrator.rs`, and `frontend/src/sidebar/SpatialDashboard.tsx`: existing implementation reviewed 2026-10-19.
