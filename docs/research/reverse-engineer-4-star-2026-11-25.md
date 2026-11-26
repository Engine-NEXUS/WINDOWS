# Reverse engineer: real-site evaluation, open-source sources and the live run (2026-11-25)

Goal set by the owner: lift "reverse engineer this website" and Maps to 4 stars or more. Result for this feature: **about 69 % (3.5 stars), short of 4.0**. What is missing is listed in section 6. Change record: `docs/changes/209-reverse-engineer-guard-named-sites-and-evaluation.md`.

## 1. Method

* **Labels.** A helper agent that never ran or read the scanner wrote ground truth for real public sites, citing a source for every "must" technology (first-party repository file, vendor case study, engineering blog, or the page's own generator tag or header, marked as such). Labels are deliberately minimal, so "found but not labelled" is **not** an error and no precision figure is read from them.
* **Capture.** `cargo run --example site_capture -- <url> --out <dir>`: one guarded fetch per site, saved as `page.json`. `--eval <dir>` prints recall with a 95 % Wilson interval, hard errors ("must not" appeared), missing thin-page reasons and contradictions inside single-valued categories (meta-framework, CMS, e-commerce).
* **Sets.** 55 sites used for tuning (batch 1; the planned 22/33 split was abandoned after I saw held-out details, so all 55 count as tuned). 30 further sites (batch 2) labelled afterwards, 29 captured (theguardian.com did not serve HTML), **evaluated once, after the last scanner change**.
* The captures and labels are kept outside the repository (about 40 MB of third-party pages).

## 2. Results (static scan only; the hidden render is not replayed offline)

| Set | Scanner | Labelled technologies found | Recall (95 % CI) | Hard errors | Contradictions |
|---|---|---|---|---|---|
| 55 tuned sites, first baseline (22-site dev half only) | old | 19 / 26 | 73.1 % (54 to 86) | 0 | not measured |
| 55 tuned sites | new | 70 / 78 | 89.7 % (81 to 95); headline categories 93.8 % | 0 | 0 of 29 |
| **29 unseen sites** | old | 24 / 33 | 72.7 % (56 to 85) | 1 | 2 of 18 |
| **29 unseen sites** | **new** | **25 / 33** | **75.8 % (59 to 87)**; headline 85.2 % (68 to 94) | 1 | 2 of 19 |

**Read this honestly:** on unseen sites the new scanner found one more technology than the old one. That is within noise. Most of the gain on the tuned set did not carry over, because it came from fixing the specific technologies those sites use.

Unseen misses: server frameworks that a page does not reveal (Rails on github.com, Django on mozilla.org, python.org, simonwillison.net), a headless WordPress (time.com), Drupal on usa.gov, Hugo on docs.docker.com, BigCommerce behind a custom front end (zogics.com). Hard error: reactnative.dev reported as Next.js. Contradictions: Elementor (a WordPress page builder) is filed under `cms`, so two WooCommerce shops show a second "CMS" (follow-up: move it to a plugin category). x.com did not report the empty-shell reason.

## 3. Changes the evaluation caused

* Missing signals added: WooCommerce generator tag (with version), Framer (generator, `Server: Framer`, comment), MkDocs theme markers, PHP page links, Azure App Service cookie.
* Missing technologies added: Eleventy, Docsify, TYPO3, Statamic, Craft CMS, WordPress VIP, Varnish, Super (`scripts/patch_fingerprints_2026_11_25.py`, repeatable).
* Wrong categories fixed: jQuery was a "frontend framework", lodash and Moment.js "programming languages", routers and Next.js sub-features "frameworks".
* A weakly implied framework (React implied by shadcn/ui on a Svelte site) is dropped when another real framework is there.
* A generator meta tag with no table entry is now reported by its own name and version.
* A tiny "check your browser" page served with HTTP 200 is reported as such instead of guessed.

## 4. Open-source sources considered

| Source | Licence | Decision |
|---|---|---|
| Wappalyzer and its forks (`enthec/webappanalyzer`, `HTTPArchive/wappalyzer`, `dochne/wappalyzer`) | GPL-3.0 | Not used (no GPL in NEXUS) |
| `projectdiscovery/wappalyzergo` | MIT repository, but the data descends from Wappalyzer's | Not used: the provenance of later updates is unclear |
| `RetireJS/retire.js`, `Library-Detector-for-Chrome` | Apache-2.0, MIT | Already imported earlier |
| `projectdiscovery/nuclei-templates` `http/technologies` | MIT | Imported with strict rules (below) |

nuclei-templates import (`scripts/import_nuclei_fingerprints.py`, pinned commit `c136408aab5ac8a31860fece88c557bd57cae541`): the first, broad conversion added 461 products and produced **74 false detections on the 55 sites** ("Documentation" became "Symfony Default Page", `/themes/` became "Winter CMS", a generic generator regex became "Metatag CMS"). Rejected. The shipped version requires the product's own name inside every matched text and skips page-type names: 352 products. Its value is breadth for less common products; **no accuracy gain from it was measured** on either site set.

## 5. DNS rebinding in the hidden render

WebView2 resolves names itself, so the static probe's address pin did not reach the render. The render now starts with its own throwaway profile and `--proxy-server` pointing at `render_proxy.rs`: loopback only, `CONNECT` to port 443 only, the scanner's URL guard on every target, every resolved address must be public, and the tunnel connects to an address that was checked. Live: 4 to 74 tunnels per site through the real WebView2, none refused. Not proven live: a rebinding attempt itself (covered by unit tests with a fake resolver). Plain `http` sub-resources are now refused.

## 6. What is still missing for 4 stars

1. **The tour has never run live**: it needs a browser in front and a Gemini key, and the test profile holds no keys. Owner session: say "reverse engineer this website" with a site open in Brave.
2. The browser hand-off (new window beside the sidebar), the Up/Down depth keys, display scaling and the desktop-app scan: not run.
3. Owner's own voice: not run.
4. Accuracy on unseen sites is 76 %; server-side frameworks mostly cannot be seen from one anonymous fetch.
5. "stripe.com" is refused as a banking site although only its public page would be read (safety rule left unchanged, owner's decision).
