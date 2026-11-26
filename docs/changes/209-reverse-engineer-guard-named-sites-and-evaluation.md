# 209: Reverse engineer: guarded render, named sites, measured accuracy, live run (2026-11-25)

Research and numbers: `docs/research/reverse-engineer-4-star-2026-11-25.md`. Maps work of the same day: change 208.

## What changed

* **Named sites.** "Reverse engineer nextjs.org", "what is stripe dot com built with", "how did they build linear.app" scan that site without it being open. Known top-level domains only (so `next.js`, e-mail addresses and private names are never taken for a site). A named site is toured only when it is already in the front browser window, so screenshots are never taken of an unrelated window.
* **Render guard** (`render_proxy.rs`): every request of the hidden render leaves through a loopback proxy that allows `CONNECT` to port 443 only and requires every resolved address to be public.
* **Fingerprints:** gaps and wrong categories fixed from real sites; 352 products imported from nuclei-templates (MIT) under strict rules.
* **Analysis:** unlisted generator tags are reported; weakly implied conflicting frameworks are dropped; tiny browser-check pages are reported.
* **Evaluation tooling:** `site_capture --eval`, recall with Wilson intervals, contradictions, `scripts/drive-isolated-turns.mjs` for live runs.

## Defects found in the live run and fixed

1. A named site still required a window in front: all nine turns of the first run answered "I couldn't identify the foreground window".
2. With no tour, the closing line was spoken 4 ms after the summary and cut it off.
3. A refused scan showed "0 of 6 security headers" and an empty weight chart as if they were findings; the sidebar now shows only the reason.

## Evidence

* Library tests: 3,640 passed, 0 failed, 34 ignored (26 voice tests filtered out as usual); then 353 focused tests including the new ones.
* Frontend: 1,296 tests (one icon-coverage failure from the new categories, fixed; sidebar suite 151 of 151), `tsc` clean.
* Mutations: `docs/testing/runs/reverse-engineer-mutations-20261125.json`.
* Live (isolated contributor build, hidden desktop, quiet mode, the owner's NEXUS untouched): `docs/testing/runs/reverse-and-maps-live-20261125.md`.

## Not verified

The screenshot-and-narrate tour, the browser hand-off, depth keys, desktop-app scan, display scaling, the owner's voice, a real DNS-rebinding attempt against the live render, the dead-code ratchet on a runner.
