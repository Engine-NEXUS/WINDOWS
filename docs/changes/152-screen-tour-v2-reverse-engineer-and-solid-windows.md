# 152 — Screen tour v2, "reverse engineer this website/app", and solid panel windows (2026-10-21)

Plan: `C:\Users\Chitkul Lakshya\.claude\plans\ticklish-crafting-shell.md`. Code was written by `agy` (Gemini) and free OpenCode models from written specs; every diff was reviewed and every claim below was re-checked by running the tests/gates myself.

## Why

Live log of "analyze my screen": the vision call timed out every time and the answer fell back to OCR ("8 text lines, starting with: o"). Cause: `vision::shared_vision_client()` had a **4 s** request timeout and `screen_tour::TOUR_TOTAL_TIMEOUT_SECS` was **6**; a tour answer needs 6-20 s. Separately, panel windows looked transparent/blurred in the pitch-black/light themes.

## What changed

1. **Vision call can finish** (`vision.rs`, `screen_tour.rs`, `orchestrator.rs`): tour-only client (20 s per request, 25 s for the whole model ladder); one "Give me a minute, sir." after 6 s; if vision still fails, the OCR text is explained by the free text router instead of reading junk (`explain_screen_text`, `ocr_spoken_summary`).
2. **Tour v2**: the Gemini script now carries `draw` (ring, underline, bracket, circle, arrow, highlight, spotlight, scan, lens, connect, pin), `link_to`, `intent` (explain, find, compare, summarize, decide, debug) and `stack_refs`. Rust validates (lens only on small boxes, one spotlight, connect needs a link). The stage draws each style with a dark halo so it reads on light and dark pages; gliding pointer, blur-fade card, intent chip, progress strip (`frontend/src/stage/annotations/*`, `tour.css`). Magic UI (Border Beam, Animated Beam, Ripple, Pointer) was used as design inspiration only: no package or source copied.
3. **Zoom in the sidebar**: hover magnifier and a pan/zoom lightbox (`Lens.tsx`, `ZoomViewer.tsx`, `lensMath.ts`), no new dependency.
4. **Solid windows**: about 20 `backdrop-filter` / sub-1.0-alpha surface layers removed from the sidebar, analysis cards, architect, settings, hub, coderabbit and calibrator. `scripts/check-window-opacity.mjs` (+ `window-opacity-registry.json`) fails if one returns. The stage overlay and orb stay transparent on purpose. **This supersedes the 2026-10-15 "liquid glass" directive for panel windows.**
5. **Reverse engineer** ("reverse engineer this website", "what is this site built with", "how was this site built"): `intent_parser` -> `reverse_engineer` NLU label (`center.rs`) -> `reverse_engineer::run`. `site_probe.rs` fetches the public page once (DNS-pinned, no auto-redirects, no cookies, 1.5 MB cap, refuses private/loopback/metadata addresses and sensitive pages) and matches 267 fingerprints in `site_fingerprints.json` (our own table; bare-word signals are weak and need a second signal). `app_probe.rs` reads an app's install folder only. `reverse_scroll.rs` holds the pure layout/scroll logic. The runner moves the browser to the right, docks the new left sidebar `stack` view (`StackView.tsx`), scrolls viewport by viewport, narrates and draws each part, highlights the matching technology card (`reverse:focus_card`), and restores the window.

## Bugs found by cross-checking (all fixed)

- Fingerprint table: ~90 weak signals matched everyday words ("moment", "medium", "drift") at 0.7; they now need corroboration.
- "analyse the tech stack of this website" was claimed as a GitHub repo analysis.
- **`::ffff:127.0.0.1` (IPv4 disguised as IPv6) was judged public by the first scanner draft** (SSRF hole). Fixed; regression test `disguised_ipv4_inside_ipv6_is_judged_by_the_embedded_address`.
- Runner: a crashed scan task skipped the cleanup; a refused scan left the sidebar on "scanning" forever.
- Dead-code gate: three unused items of ours removed.

## Verified (run by me)

Frontend `tsc` clean, vitest 63 files, window-opacity guard + self-test, fingerprint table checks (267, no duplicates, regexes compile), full Rust lib suite **2429 passed, 0 failed**, turn-end and hygiene gates OK. The 11 drawing styles, the full overlay with mocked events, and the Tech Stack view (dark and light) were looked at in a browser harness (`/dev-annotations.html`, `/dev-stack.html`, dev server only, not shipped).
IPC gate and the dead-code gate still fail on items that are not ours (comment lines inside `generate_handler!`, `agent_followup`, `tts`, `vision` fields); nothing from this work is listed.

## NOT verified (needs the owner)

Real voice runs: a live Gemini answer and its latency, drawing alignment at 125/150 % scaling, speech/callout timing, the browser move/scroll/restore on real windows (Brave tested nowhere yet), the scroll step on smooth-scrolling pages, `Lens` in the real sidebar, whether any panel still looks see-through (the guard covers only the surfaces it lists), and the two-site reverse-engineer walkthrough (one Next.js, one WordPress). A website cannot be fully reverse engineered from outside: server code and databases are invisible, and the report says so.

## Side findings (not changed)

STT turned silence into "Thank you very much, Mr. Green" (hallucination filter miss); Google shows "this device isn't linked to the NEXUS cloud service".
