# 6. Reverse engineer website / app

Scorecard before: Built 85, Tested 65, Live 0, Safety 80, Industry 40 (2.5 stars). Source and offline only so far; **no live scan has ever succeeded**.

## What it is

"Nexus, reverse engineer this website" opens the site in its own browser window, scans its technology stack (headers, HTML, scripts, a hidden render for single-page apps) against a fingerprint table (now 369 entries, `site_fingerprints.json`), shows the result in the left sidebar and scrolls and narrates the page. For an app it inspects the install folder. Files: `reverse_engineer.rs`, `site_probe.rs`, `site_render.rs`, `app_probe.rs`, `reverse_scroll.rs`, `scan_window.rs`, `site_eval.rs`.

## Verification (what was found)

Baseline: 134 focused tests pass (scanner, hidden render, accuracy replay, scroll stepper, runner outcomes, phrasings). But the accuracy gate, `real_site_accuracy`, ran on **4 synthetic fixtures** (`baseline.json` says so): **no real-site accuracy existed**.

### A real-site check (new)

Eight public sites were fetched once each with the project's own capture tool (`site_capture`: guarded fetch, public hosts only, no cookies) and replayed offline: nextjs.org, tailwindcss.com, vuejs.org, svelte.dev, astro.build, gatsbyjs.com, remix.run, wordpress.org.

| Site | Headline result |
|---|---|
| nextjs.org | Next.js, Vercel, React: all correct |
| tailwindcss.com | Next.js, Vercel, React, Cloudflare: correct |
| svelte.dev | SvelteKit, Svelte, Vercel: correct |
| astro.build | Astro, Cloudflare: correct |
| wordpress.org | WordPress: correct |
| gatsbyjs.com | Gatsby, Netlify, React: correct. **A category error:** "Gatsby Image" is filed under `cms` |
| vuejs.org | Vue, Netlify: correct. **VitePress missed**: the page says `<meta name="generator" content="VitePress v2.0.0-alpha.17">` and the table had no VitePress entry |
| remix.run | Fly.io, Fastly, Alpine.js found; **Remix not found**. No ground truth was labelled for this site (its current stack is not stated anywhere I could cite), so it is not scored |

So the headline stack was right for 6 of 8 sites without any change, and the one clear miss was a **gap in the table found by real data**, not by the synthetic fixtures.

### Other finding (advisor, confirmed)

`site_probe.rs` built its HTTP client **without `.no_proxy()`**. The scanner's SSRF guard resolves the name, requires a public address and pins the connection to it; under a system or environment proxy the **proxy** resolves the name itself, so the pin does nothing and a private address on the proxy's side could be reached.

## What was built

* **Fingerprints** for VitePress (observed on the real page: generator meta, `VPApp` root, `VPNav` classes) and the other documentation / static-site generators that state themselves in the standard generator meta: Docusaurus, Hugo, Jekyll, MkDocs. (The last four were added from their documented generator tags, not from a captured page: they are untested against a real page.)
* **`.no_proxy()`** on the scanner client, pinned by a test.
* **Three real fixtures** kept in `tests/fixtures/site_real/` (svelte.dev, vuejs.org, wordpress.org: 780 KB in all), each with `labels.json` whose `must` entries cite the site's own statement. The fixtures are replayed by the existing `real_site_accuracy` test. **vuejs.org is a tuned fixture** (the table was changed because of it) and is labelled so; svelte.dev and wordpress.org were not used to change anything and act as held-out.
* Result of the replay (these 3 real sites plus the 4 synthetic ones; the counts below mix both, so they are not a real-site figure on their own): every headline category scored F1 1.0 with **0 misses** (meta-framework tp 3, frontend-framework 4, hosting 2, cdn 1, cms 2, ecommerce 1). For the 3 real sites alone, every labelled "must" was found. The non-headline "false positives" (google-tag-manager, x-frame-options, csp, svg-sprite, pwa, fathom) are real detections that were simply not listed in `may`; the labels are deliberately minimal, so those categories show low F1 and are not gated.

## Not done, and why

* **n = 3 real sites is not a measurement.** The plan's target is 50 sites (`docs/research/capabilities-audit/03-reverse-engineering.md` §7). More captures are cheap (one command each) but each needs a label from the site's own statement. `remix.run`'s current stack and `gatsbyjs.com`'s category oddity were left for that pass; the Gatsby Image category should move to `frontend-framework`.
* **The hidden render on a real WebView2 and the live scan** (Brave hand-off, F11 address read, depth keys, a Cloudflare-protected site, one desktop app) need the owner's machine: the harness would force-stop the running NEXUS.
* The remaining 8 safety mutations of change 197 are unchanged.

## Owner checklist

1. Rebuild, then say "reverse engineer this website" on a Next.js site, a WordPress site, a Cloudflare-protected site. Log lines to read: `[REVERSE] url diag:`, `[REVERSE] icons:`.
2. Say it on a desktop app's window (the app folder scan).
3. For the 50-site set: `cargo run --example site_capture -- https://<site>`, then label from the site's own statement.
