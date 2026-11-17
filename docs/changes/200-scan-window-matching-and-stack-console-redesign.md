# 200 - Scan window matching fix and the Tech Stack "Console" redesign

Date: 2026-11-16. Source edits and offline tests. Not proven live (see the end).

## What the owner reported (live run on build 2754b8c)
- `[REVERSE] place: no new browser window appeared, scanning in place`: Brave did open a new window, NEXUS did not
  recognise it, so it was never sized beside the sidebar.
- The sidebar scrolled with a scrollbar; the owner wanted natural scrolling.
- The first redesign was not good enough; the owner supplied four reference designs (dark charcoal and light grey app
  dashboards) and asked for a simple, minimal, mostly black-and-white interface.

## Changes
1. **`scan_window.rs`**: the new window is found by the browser's exe stem, by any known browser exe (a launcher can
   hand the request to another process) or by the foreground window when it is a browser window that did not exist
   before. The wait is about 10 s. If nothing matches, one log line lists what appeared (exe, class, title length).
2. **`kit/Card.tsx`**: an optional `id`, used as scroll anchors.
3. **`StackView.tsx` + `stack-v2.css`** (all surfaces solid, window-opacity gate passes):
   - no scrollbars (wheel, touch and trackpad scroll, soft fade at the edges), cards use the whole window width;
   - hero: overlapping logo stack, headline, 2x2 KPI tiles (technologies, scripts, hosts, security), copy button;
   - stack mix: donut beside grey bar rows; security: score ring and header bars;
   - technologies grouped by category, each in its own rounded card with a large logo, version, the first piece of
     evidence and a confidence bar with a percentage; search box and a filled-pill filter (All / Confirmed / Likely /
     Possible);
   - icon tab bar (Overview, Stack, Security, Weight) that scrolls to each card;
   - colour only in the logos; everything else black, white and grey, dark and light themes.
4. The stack view test expects the KPI tiles instead of the old one-line summary.

## Verify
Frontend tsc clean, vitest 1230/1230, window-opacity and IPC gates pass, Rust `scan_window` tests pass.

## Not verified live
The latest NEXUS log (build fbd1f28) contains no scan: it ends with the owner's spoken "shut down the laptop", which
NEXUS executed. So the new-window placement, the depth keys and the redesign have not been seen in a real run. They
were checked only in the browser preview and by unit tests.
