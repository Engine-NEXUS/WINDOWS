# Reverse Lab - real-world test (programs, Electron bundles, packed files, websites/APIs) - 2026-11-27

Everything here ran on real third-party software and live public sites from the Linux sandbox. Nothing was executed (programs were only read).
Re-run: `python scripts/reverse_live/real_apps.py <folder>` and `python scripts/reverse_live/score_sites.py <dev|second|fresh>` (kit in `scripts/reverse_live/`).

## 1. Real Windows programs (74 PE files from 13 vendor downloads)

7-Zip 24.08, Notepad++ 8.6.7 (+ plugins, GUP, libcurl), PuTTY 0.85, Node.js 20.11 (68 MB), Electron 30 / Chromium (173 MB exe + DLLs), Python 3.12 embeddable (incl. OpenSSL, SQLite), SQLite tools, Sysinternals Autoruns / ProcDump / PsTools (26 exes), Newtonsoft.Json (8 .NET builds), UPX.

| Measure | Result |
|---|---|
| Imports + section count vs **pefile** (independent oracle) | **74 / 74 files identical** (0 mismatches), incl. 8 .NET assemblies |
| Time per file | mean **82 ms**, 173 MB Electron/Chromium exe 1.07 s, 68 MB node.exe 0.91 s |
| Packed verdict on unpacked vendor software | 73 of 74 "none"; the one flagged was UPX's own `upx.exe` (correct) |
| Capability findings that make sense | Autoruns: persist via Run key, IFEO hijack (it edits IFEO), downloader; ProcDump: debug privilege; PsExec/PsTools: install a service; psloglist: clear event logs; pskill/pssuspend: debug privilege |
| **Noisy findings** | **High severity on legitimate software**: Autoruns (6 files, "IFEO hijack") and Electron/Chromium ("inject code / DLL into another process" - Chromium really imports those APIs for crash dumps and sandboxing). The report says "capability, not intent", but a red "high" on a browser engine is misleading. Not fixed yet: severity does not consider signature or vendor. |

## 2. Packed files (real UPX output of real programs)

11 real programs/DLLs (PuTTY, 7-Zip, Notepad++, sqlite3, ProcDump, PsExec, Autorunsc, python312.dll, libcurl, libcrypto, node.exe) packed with UPX 4.2.2, each in 3 variants: original section names, names blanked, names disguised as `.text/.data/.rsrc`.
**33 / 33 flagged "likely packed"** (UPX named correctly where the names survive); the scrubbed variants are caught by structure (empty executable section, high-entropy code, writable entry point). node.exe (68 MB) takes 0.35-0.41 s.
Limitation shown by this test: once packed, the real imports vanish, so capability findings (e.g. ProcDump's debug privilege) disappear - which is exactly why the report prints the packed warning first.

## 3. Electron bundles

`app.asar` built by the official `@electron/asar` from a real app (this repo's frontend: 8,584 files, 281 MB archive) and a small app with a native module, plus Electron's own `default_app.asar`.
**File count 8,584 = 8,584 and native modules 5 = 5 versus the official tool's own listing**; package name/version/main/dependencies read correctly. 0.45 s for the 281 MB archive, 20 ms for the small ones.
Not tested: a real shipped Electron product's `app.asar` (no download source was reachable from the sandbox).

## 4. Real websites and APIs (browser HAR -> OpenAPI)

HARs recorded with headless Chromium (Playwright), driving each site like an app (page load + `fetch` calls, a few with planted Bearer tokens / API keys / passwords). Ground truth = the route templates of the calls driven (from each API's documentation; Petstore also has its official OpenAPI spec). Three sets, because the tool was fixed in between:

| Set | Sites | Endpoint F1 | Per-request template accuracy | Meaning |
|---|---|---|---|---|
| dev | Petstore v3, JSONPlaceholder, DummyJSON, PokeAPI, httpbin (+ HN firebase, Wikipedia) | 0.97 / 1.00 / 1.00 / 1.00 / 0.90 (the last two sites could not be measured, see below) | 97-100 % | **tuned on these** after a first failing look |
| second | Open Library, catfact, SWAPI, dog.ceo, zippopotam, reqres, TheMealDB, agify | **0.965** (first look: **0.61**) | **1.000** (first look: **0.42**) | tuned after its own first look |
| **fresh** | Rick & Morty, TVmaze, FakeStore, Art Institute of Chicago, HN Algolia, genderize, nationalize | **0.889** | **0.964 (80 / 83)** | **scored once, code frozen - the only independent number** |

- Speed: 3-70 ms per 1-4 MB HAR. Secrets: planted tokens, API keys, passwords and cookies appeared in **0 of 22 outputs**.
- Petstore vs its **official OpenAPI spec**: all 19 operations recovered once `findByTags` (which answers `[]`) stopped being folded into `/pet/{id}`.
- Auth detected: Bearer, Basic, API-key header, cookie. Pagination detected: limit/offset/skip.
- What dragged the fresh F1 down: (a) endpoints the **page itself** called (Gatsby `page-data`, a `/graphql` call) are real requests that my ground truth did not list - counted as false positives; (b) HN Algolia `/users/{pg,sama,dang}`: three user pages stayed literal because the evidence rules did not match their responses.
- Not measurable here: Hacker News firebase (the browser could not fetch it) and Wikipedia (the sandbox IP was rate-limited, every answer was HTTP 429, so there were no response shapes to learn from - an error-only capture cannot be generalised).
- Plain HTML sites (python.org, example.com): static assets and analytics are skipped; python.org's own XHRs (status page, `/authenticated`) are found; example.com correctly reports "no API requests".

### What this testing found and fixed (all with unit tests)
1. Fetches that answered with an empty/HTML body (`/status/200`, rate-limit pages) were thrown away as "static"; the browser's `_resourceType` / `Sec-Fetch-Dest` are now used.
2. `rest_v1`, `oauth2`, `sha256` were treated as ids and replaced by `{id}`.
3. A lone word value (`/pokemon-species/mew`) never joined its numeric siblings; now it does when its responses agree (reserved words like `me`, `search` never do).
4. A "same-shape" rule folded real routes (`search.php`, `lookup.php`, `filter.php` all answer `{"meals": ...}`) - routes that accept different query parameters are no longer merged.
5. Whole route families were collapsed into `{}` when their (empty) redirects gave no evidence; a family now needs comparable, non-empty responses.
6. Ids with a data suffix (`OL45804W.json`, `{id}.json`) are templated with the suffix kept.
7. Cloudflare / analytics / error-reporting beacons are no longer endpoints.
Regression check after every change: the mock-API benchmark stayed at F1 0.9966 (held-out seeds), template accuracy 0.9988, 0 leaks; 87 reverse-core unit tests pass.

## 5. Honest summary
Strong: PE/.NET parsing fidelity, speed (even 170 MB binaries in about a second), packed-file detection, asar parsing, secret handling.
Medium: HAR -> OpenAPI generalises to about 0.89 F1 / 96 % template accuracy on unseen public APIs; page-own XHRs and sparse or error-only captures are the weak spots.
Weak / open: severity is context-blind (legit tools show "high"); no real shipped Electron product, no real malware, no real LLM and no Windows live flow were tested.
