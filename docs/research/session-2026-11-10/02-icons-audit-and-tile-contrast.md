# Icons: audit of the bundled set, the Antigravity/Gemini/Kilo fixes, and tile contrast (2026-11-10)

## 1. The report

The owner saw an old Antigravity icon, wanted every icon deleted and re-downloaded from a free icon service, and asked
for a cross-check and a plan first.

## 2. What was in the app

`frontend/public/activity-logos/` holds 95 SVG marks (98 files with the README, `SOURCES.json` and a licence file).
They are bundled locally: nothing is fetched at runtime. Provenance is recorded in `SOURCES.json` (source, domain or
slug, hash) and enforced by `scripts/check-activity-icons.mjs` (every file has an entry, hashes match, no external
references). 73 of the 95 were recorded only as "existing (theSVG / Iconify / Lobe Icons / NEXUS)", 21 came from
logoapi.sh, none from an official brand kit.

## 3. Why the Antigravity icon looked wrong

`antigravity.svg` was a broken copy from an icon pack: the coloured blobs were not masked and the viewBox cropped the
mark, so it rendered as a block of colours instead of the arch. Rendered side by side on dark and light tiles it was
unmistakably wrong; the file had a source record of just "existing", so nothing could notice.

Fix: the current mark taken from the product's own site, `https://antigravity.google/favicon.svg` (a blue arch with a
yellow/red/green gradient). The favicon's background circle and its dark-mode `<style>` were removed and the viewBox
cropped to the mark. Source URL, domain, date, a note on the edits and the hash are in `SOURCES.json`.

Hash pitfall found by CI: the first hash was computed from the Windows working copy (CRLF) while the repository stores
LF (`.gitattributes: * text=auto eol=lf`), so `frontend-check` failed with "sha mismatch". Record hashes of
`git show HEAD:<file>`, not of the working copy.

## 4. Review of the whole set (method)

Not a blind re-download: a contact sheet of all 95 on light and dark backgrounds, next to every candidate that could be
fetched automatically.

- Candidates: the product site's own SVG favicon (found for 49 ids), Iconify `logos` / `simple-icons` (the free Iconify
  endpoint started returning Cloudflare error 1015 partway), and the Logo.dev connector (search costs about 0.001
  credit, a full brand lookup 5 credits; its main logo for Antigravity was a 200x184 PNG and its SVG only a wordmark).
- 46 icons (Slack, Spotify, Teams, Steam, Notion, and others) had no automatic candidate, so they were judged by eye only.

Result: **most of the set was already the correct current mark and was kept.** The fetched official favicons were often
worse: Android Studio's came back as the plain Android robot, Netlify's in a different style, several with filled
backgrounds or tiny sizes. A blanket replace would have made the set worse.

Changed:

| Icon | Problem | Fix |
|---|---|---|
| antigravity | cropped, unmasked blobs | official favicon mark, cleaned |
| gemini | flat purple star (Simple Icons colour) | same star path with the official blue-purple-red gradient (our own gradient on the star, not a file from Google) |
| kilo | black mark | official yellow mark from `kilocode.ai/favicon.svg` |

Left alone, undecided: Devin (the site favicon is a simpler mark than ours; unsure which is current) and Netlify
(favicon differs slightly from ours).

Not used: the `iconstack` Supabase function the owner pasted. The Logo.dev connector already available was enough, and
the second source adds a provider without a licence check.

## 5. Tile contrast (the larger problem)

Many marks were drawn on a tile of their own colour: Claude's orange mark on an orange tile, Supabase green on green,
black marks (ChatGPT, OpenAI, Notion, Zed, Render, Vercel) on black. The files are correct; the tile table
(`activityIconColors.json`, the *brand* colour) was being used as the tile background by the toast and Hub tiles.

Fix: `scripts/gen-icon-tiles.mjs` computes, per icon, the tile that makes the mark visible and writes
`frontend/src/settings-sidebar/hub/activityIconTiles.json`:

1. the brand colour if the mark contrasts with it by at least 3:1 (white marks: GitHub, Vercel, WhatsApp...);
2. else a light tile `#f4f4f6` if the best paint colour contrasts by at least 1.8:1;
3. else a dark tile `#1c1c21`.

Paint colours are read from `fill`/`stroke` (hex, `white`, `black`, `currentColor` = black in an `<img>`, `url(#..)`
gradient = a mid-tone). `tileColor()` in `notify-toast/ProviderLogo.tsx` uses the generated table first. A test renders
the contrast for every icon id with a tile; CI runs the generator with `--check` so a new icon cannot ship invisible.

Pitfall met while writing the script: backslash escapes inside a Python heredoc passed through the shell tool turned
`\b` into a literal backspace character in the regex, so no paint colour matched and every icon looked black. Check the
generated file for sane values (a printout of claude/github/whatsapp tiles caught it).

Verification: contact sheet of all 95 on their new tiles, every mark visible; icon gate (95 logos); 1,127 frontend tests;
Agent Island end-to-end run (12 checks) with Claude's mark visible on its dark tile.

## 6. Not verified

- The Antigravity icon inside the running island (the island demo has two agents, neither Antigravity).
- Toasts and the Hub tile in the running app (only rendered on a sheet).
- Netlify's "n" on teal and Tailscale's dots on light are the closest contrast calls.

## 7. Follow-ups

- Decide Devin and Netlify with the owner.
- Replace the "existing (...)" source records for the other 73 with a real URL and date when each is next checked.
- A small dev page that renders the island, a toast and a Hub tile for a chosen agent so every icon can be seen in
  context without a live session.
