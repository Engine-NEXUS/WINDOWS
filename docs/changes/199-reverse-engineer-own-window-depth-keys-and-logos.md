# 199 - Reverse engineer: its own browser window, Up/Down depth keys, logos for everything

Date: 2026-11-15. Source edits and offline tests; needs a rebuild. Not run live.

## What the owner reported
- After a scan the browser ended up full screen again.
- They wanted the page in a new Brave window attached beside the sidebar, their own window left alone.
- The Tech Stack sidebar did not show the icons for the software and languages it named.
- Up/Down arrows should send the sidebar behind / in front of every window, like Left/Right move it sideways.

## Changes
1. **Own window** (`scan_window.rs`, `reverse_engineer.rs`): the site opens in a NEW window of the same browser
   (`--new-window`), found by exe plus a handle that did not exist before (not by pid: Chromium hands the request to
   the running instance). It is sized from the sidebar's REAL right edge (`reverse_scroll::plan_page_rect_beside`),
   so the pair is flush at any display scale. The old planner assumed 540 logical px, wrong at 125/150 %.
   The original window is never moved. The restore step (which called `SW_MAXIMIZE` on a window that started
   maximised) is deleted, and a test fails if reverse_engineer.rs ever mentions maximise/restore/set_fullscreen.
   If the new window does not appear or the screen is too small, the scan runs in place and moves nothing.
2. **Depth keys** (`dockKeys.ts`, `commands.rs::set_sidebar_depth`): ArrowDown sends the sidebar to the bottom of the
   window stack (no longer topmost), ArrowUp raises it topmost again. A "sent back" flag stops `finish_sidebar` from
   focusing it forward on the next dock or update. The scan end calls `raise_sidebar`, which clears the flag.
3. **Logos** (see below), plus per-item glyphs for headers/hints and icons in the text chips.

## Logo sources (all bundled; no key, no network at run time)
- `scripts/vendor-tech-icons.py`: simple-icons 13.21.0 (CC0), @iconify-json/logos 1.2.17 (CC0), devicon (MIT),
  vscode-icons (MIT), cib (CC0), plus the app's own activity-logos. 388 marks; one `techIcons.json` table.
- `scripts/scrape-tech-icons.py`: for the tools with no brand set, downloads the mark from the tool's own site (svg
  link, apple-touch-icon, favicon). 53 marks into `public/tech-icons/scraped/`, listed in `techIconsScraped.json`
  with the source URL. Generic host icons (GitHub, Bitbucket, a news site...) are rejected so a wrong logo is never
  shown. Marks belong to their owners; presence implies no affiliation.
- Everything else gets a glyph (`kit/techIcon.tsx`): per-id for headers and hints (lock, bolt, box), per-category
  otherwise. A letter tile only appears for an unknown category. A test fails if any fingerprint or app-scan id has
  neither a logo nor a glyph.
- Runtime: `brand_logo::keyless_logo` fetches a site's `apple-touch-icon.png` (no key) when Logo.dev gives nothing.
  `[REVERSE] icons: n/m bundled, no logo for: ...` is logged for every site scan.

## Verify
Frontend: tsc clean, vitest 1070/1070. Rust: scan_window, reverse_scroll, reverse_engineer, brand_logo, depth tests
pass; IPC, turn-end and hygiene gates pass; dead-code ratchet shows only the 2 `desktop_control.rs` items that belong
to other work.

## Not verified
Real Brave hand-off timing, window placement at 125/150 %, multi-monitor, depth keys on a real desktop, the scraped
logos' look at 14-32 px on real data, the keyless favicon on live sites.
